use anyhow::{Context, Result};
use ffmpeg_next as ffmpeg;
use std::path::Path;

pub struct VideoDecoder {
    ictx: ffmpeg::format::context::Input,
    video_stream_index: usize,
    decoder: ffmpeg::decoder::Video,
    scaler: ffmpeg::software::scaling::Context,
    target_width: u32,
    target_height: u32,
}

// Safety: FFmpeg contexts are moveable between threads.
// We ensure it is only used by one thread at a time.
unsafe impl Send for VideoDecoder {}
unsafe impl Sync for VideoDecoder {}

impl VideoDecoder {
    pub fn new<P: AsRef<Path>>(path: P, target_width: u32, target_height: u32) -> Result<Self> {
        ffmpeg::init().context("Failed to initialize FFmpeg")?;

        let ictx = ffmpeg::format::input(&path).context("Failed to open input file")?;
        let input = ictx
            .streams()
            .best(ffmpeg::media::Type::Video)
            .ok_or_else(|| anyhow::anyhow!("No video stream found"))?;
        
        let video_stream_index = input.index();
        let context_decoder = ffmpeg::codec::context::Context::from_parameters(input.parameters())?;
        let decoder = context_decoder.decoder().video()?;

        // Scale to screen size for efficient GPU copy
        let scaler = ffmpeg::software::scaling::context::Context::get(
            decoder.format(),
            decoder.width(),
            decoder.height(),
            ffmpeg::format::Pixel::BGRA,
            target_width,
            target_height,
            ffmpeg::software::scaling::flag::Flags::FAST_BILINEAR,
        )?;

        Ok(Self {
            ictx,
            video_stream_index,
            decoder,
            scaler,
            target_width,
            target_height,
        })
    }

    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>> {
        let mut frame = ffmpeg::util::frame::Video::empty();
        
        let mut total_scanned = 0;
        while let Some((stream, packet)) = self.ictx.packets().next() {
            total_scanned += 1;
            if total_scanned > 500 {
                tracing::warn!("Decoder scanned 500 packets without finding a video frame. Returning None to prevent lockup.");
                return Ok(None);
            }

            if stream.index() == self.video_stream_index {
                self.decoder.send_packet(&packet)?;
                if self.decoder.receive_frame(&mut frame).is_ok() {
                    let mut rgb_frame = ffmpeg::util::frame::Video::empty();
                    self.scaler.run(&frame, &mut rgb_frame)?;
                    return Ok(Some(rgb_frame.data(0).to_vec()));
                }
            }
        }

        // Handle loop? For now just return None to signal EOF or restart outside
        Ok(None)
    }

    pub fn seek_to_start(&mut self) -> Result<()> {
        self.ictx.seek(0, ..0)?;
        self.decoder.flush();
        Ok(())
    }

    pub fn width(&self) -> u32 {
        self.target_width
    }

    pub fn height(&self) -> u32 {
        self.target_height
    }
}
