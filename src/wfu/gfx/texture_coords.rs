use wfu::util::first_greater_power_of_two;

#[derive(Copy, Clone)]
pub struct TextureCoords {
    pub left: f32,
    pub bottom: f32,
    pub right: f32,
    pub top: f32,
}

impl TextureCoords {
    pub fn from(img_width: i16, img_height: i16, flip: bool) -> TextureCoords {
        let width = first_greater_power_of_two(img_width.into()) as f32;
        let height = first_greater_power_of_two(img_height.into()) as f32 - 0.5f32;
        let right = img_width as f32 / width;
        let bottom = img_height as f32 / height;
        if flip {
            TextureCoords {
                left: right,
                bottom,
                right: 0f32,
                top: 0f32,
            }
        } else {
            TextureCoords {
                left: 0f32,
                bottom,
                right,
                top: 0f32,
            }
        }
    }

    pub fn compute(
        coords: &[i16],
        img_width: i16,
        img_height: i16,
        img_width_total: i16,
        img_height_total: i16,
        flip: bool,
    ) -> Vec<TextureCoords> {
        let width_total = first_greater_power_of_two(img_width_total.into()) as f32;
        let height_total = first_greater_power_of_two(img_height_total.into()) as f32 - 0.5f32;
        let right = img_width as f32 / width_total;
        let bottom = img_height as f32 / height_total;
        let count = coords.len() / 2;
        let mut output: Vec<TextureCoords> = Vec::with_capacity(count);
        for i in 0..count {
            let offset_x = (coords[i * 2] as f32 + 0.5f32) / width_total;
            let offset_y = (coords[i * 2 + 1] as f32 + 0.5f32) / height_total;
            let v = if flip {
                TextureCoords {
                    left: right + offset_x,
                    bottom: bottom + offset_y,
                    right: offset_x,
                    top: offset_y,
                }
            } else {
                TextureCoords {
                    left: offset_x,
                    bottom: bottom + offset_y,
                    right: right + offset_x,
                    top: offset_y,
                }
            };
            output.push(v);
        }
        output
    }
}
