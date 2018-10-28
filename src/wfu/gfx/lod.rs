#[derive(Copy, Clone)]
pub enum LevelOfDetail {
    Low,
    Medium,
    High,
}

impl LevelOfDetail {
    pub fn get_mask(self) -> u8 {
        match self {
            LevelOfDetail::Low => 1,
            LevelOfDetail::Medium => 3,
            LevelOfDetail::High => 7,
        }
    }
}
