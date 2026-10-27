use crate::usbtmc::{CLASS_APPLICATION, IN_TRANSFER, Pipe, SUBCLASS_USBTMC, UsbTmc};
use anyhow::{Context, Result, anyhow, bail};
use nusb::MaybeFuture;
use nusb::transfer::{Bulk, Buffer, In, Out};
use nusb::descriptors::TransferType;
use nusb::transfer::Direction;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub struct UsbtmcInfo {
    pub vendor_id: u16,
    pub product_id: u16,
    pub manufacturer: String,
    pub product: String,
    pub serial: String,
    pub interface: u8,
    pub(crate) device: nusb::DeviceInfo,
}

impl UsbtmcInfo {
    pub fn label(&self) -> String {
        format!(
            "{} {:04x}:{:04x} {}",
            self.product, self.vendor_id, self.product_id, self.serial
        )
        .trim()
        .to_string()
    }
}

pub fn is_usbtmc(class: u8, subclass: u8) -> bool {
    class == CLASS_APPLICATION && subclass == SUBCLASS_USBTMC
}

pub fn list_usbtmc() -> Result<Vec<UsbtmcInfo>> {
    let mut out = Vec::new();
    for d in nusb::list_devices().wait()? {
        let Some(i) = d.interfaces().find(|i| is_usbtmc(i.class(), i.subclass())) else {
            continue;
        };
        out.push(UsbtmcInfo {
            vendor_id: d.vendor_id(),
            product_id: d.product_id(),
            manufacturer: d.manufacturer_string().unwrap_or_default().to_string(),
            product: d.product_string().unwrap_or_default().to_string(),
            serial: d.serial_number().unwrap_or_default().to_string(),
            interface: i.interface_number(),
            device: d.clone(),
        });
    }
    Ok(out)
}

pub struct NusbPipe {
    out: nusb::Endpoint<Bulk, Out>,
    inp: nusb::Endpoint<Bulk, In>,
    _interface: nusb::Interface,
}

impl Pipe for NusbPipe {
    fn write(&mut self, data: &[u8]) -> Result<()> {
        let done = self.out.transfer_blocking(Buffer::from(data), TIMEOUT);
        done.status.map_err(|e| anyhow!("USB write failed: {e}"))?;
        Ok(())
    }

    fn read(&mut self) -> Result<Vec<u8>> {
        let packet = self.inp.max_packet_size();
        let want = IN_TRANSFER.div_ceil(packet) * packet;
        let done = self.inp.transfer_blocking(self.inp.allocate(want), TIMEOUT);
        done.status.map_err(|e| anyhow!("USB read failed: {e}"))?;
        let n = done.actual_len.min(done.buffer.len());
        Ok(done.buffer[..n].to_vec())
    }
}

pub fn open_usb(info: &UsbtmcInfo) -> Result<UsbTmc<NusbPipe>> {
    let device = info.device.open().wait().context("opening the USB device")?;
    let interface = device
        .detach_and_claim_interface(info.interface)
        .wait()
        .context("claiming the USBTMC interface")?;
    let (mut out_addr, mut in_addr) = (None, None);
    if let Some(desc) = interface.descriptor() {
        for ep in desc.endpoints() {
            if ep.transfer_type() != TransferType::Bulk {
                continue;
            }
            match ep.direction() {
                Direction::Out => out_addr = out_addr.or(Some(ep.address())),
                Direction::In => in_addr = in_addr.or(Some(ep.address())),
            }
        }
    }
    let (Some(o), Some(i)) = (out_addr, in_addr) else {
        bail!("interface has no bulk in and out endpoints");
    };
    let out = interface.endpoint::<Bulk, Out>(o)?;
    let inp = interface.endpoint::<Bulk, In>(i)?;
    Ok(UsbTmc::new(NusbPipe { out, inp, _interface: interface }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usbtmc_class_match() {
        assert!(is_usbtmc(0xFE, 3));
        assert!(!is_usbtmc(0xFE, 1));
        assert!(!is_usbtmc(0x08, 3));
    }
}
