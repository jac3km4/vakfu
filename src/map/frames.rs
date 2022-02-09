use bevy::sprite::Rect;
use byte::ctx::Endian;
use byte::{BytesExt, TryRead};
use glam::Vec2;
use itertools::Itertools;

#[derive(Debug, Default)]
pub struct Frames {
    pub total_time: u32,
    pub frame_times: Vec<u16>,
    pub frame_rects: Vec<Rect>,
}

impl Frames {
    pub fn new(total_time: u32, frame_durations: &[u16], frame_coords: Vec<Rect>) -> Self {
        let mut frame_times = Vec::with_capacity(frame_durations.len());
        let mut frame_time = 0;
        for dur in frame_durations {
            frame_times.push(frame_time);
            frame_time += dur;
        }

        Self {
            total_time,
            frame_times,
            frame_rects: frame_coords,
        }
    }
}

impl<'a> TryRead<'a, u8> for Frames {
    fn try_read(bytes: &'a [u8], count: u8) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;

        let total_time: u32 = bytes.read(offset)?;
        let width: u16 = bytes.read(offset)?;
        let height: u16 = bytes.read(offset)?;
        let _width_total: u16 = bytes.read(offset)?;
        let _height_total: u16 = bytes.read(offset)?;
        let frame_durations: Vec<u16> = bytes
            .read_iter(offset, Endian::default())
            .take(count.into())
            .collect();
        let coords = bytes
            .read_iter::<i16>(offset, Endian::default())
            .take(count as usize * 2)
            .tuples()
            .map(|(x, y)| Rect {
                min: Vec2::new(x as f32, y as f32),
                max: Vec2::new(x as f32 + width as f32, y as f32 + height as f32),
            })
            .collect_vec();

        let result = Frames::new(total_time, &frame_durations, coords);
        Ok((result, *offset))
    }
}
