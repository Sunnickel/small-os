use hal::{block::BlockDevice, io::IoError};

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

    pub fn into_device(self) -> D { self.device }

    pub fn device(&mut self) -> &mut D { &mut self.device }

    pub fn size(&self) -> u64 { self.size }

    fn read_internal(&mut self, buf: &mut [u8]) -> Result<usize, IoError> {
        if self.position >= self.size {
            return Ok(0);
        }

        let to_read = core::cmp::min(buf.len(), (self.size - self.position) as usize);
        if to_read == 0 {
            return Ok(0);
        }

        let mut total_read = 0;

        while total_read < to_read {
            let sector = self.position / self.sector_size as u64;
            let sector_offset = (self.position % self.sector_size as u64) as usize;
            let remaining_in_sector = self.sector_size - sector_offset;
            let to_copy = core::cmp::min(remaining_in_sector, to_read - total_read);

            if self.buffered_sector != Some(sector) {
                self.device.read_at(
                    sector * self.sector_size as u64,
                    &mut self.sector_buffer[..self.sector_size],
                )?;
                self.buffered_sector = Some(sector);
            }

            buf[total_read..total_read + to_copy]
                .copy_from_slice(&self.sector_buffer[sector_offset..sector_offset + to_copy]);

            self.position += to_copy as u64;
            total_read += to_copy;
        }

        Ok(total_read)
    }
}
