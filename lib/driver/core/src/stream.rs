use alloc::string::ToString;

use hal::block::BlockDevice;

pub struct BlockStream<D: BlockDevice> {
    device: D,
    position: u64,
    size: u64,
    sector_buffer: [u8; 4096],
    sector_size: usize,
    buffered_sector: Option<u64>,
}

impl<D: BlockDevice> BlockStream<D> {
    pub fn new(device: D) -> Self {
        let size = device.size();
        let sector_size = device.sector_size();
        crate::debug(
            &format_args!("BLOCKSTREAM_NEW size={:#x} sector_size={}", size, sector_size)
                .to_string(),
        );
        assert!(sector_size <= 4096, "Sector size too large");

        Self {
            device,
            position: 0,
            size,
            sector_buffer: [0u8; 4096],
            sector_size,
            buffered_sector: None,
        }
    }

    pub fn into_device(self) -> D {
        crate::debug(&format_args!("BLOCKSTREAM_INTO_DEVICE").to_string());
        self.device
    }

    pub fn device(&mut self) -> &mut D {
        crate::debug(&format_args!("BLOCKSTREAM_DEVICE position={:#x}", self.position).to_string());
        &mut self.device
    }

    pub fn size(&self) -> u64 {
        crate::debug(&format_args!("BLOCKSTREAM_SIZE: {}", self.size).to_string());
        self.size
    }
}
