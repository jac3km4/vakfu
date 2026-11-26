//! Day/night sun-colour definitions parsed from `dayLight.xml`.
//!
//! The client loads a loose `dayLight.xml` from its working directory and falls
//! back to the copy embedded in `contents/data/data.jar`
//! (`WakfuClientInstance` → `SunLightModifier.readFromXML`). The file holds one
//! `<season>` per game-calendar season, each with a sorted list of `<hour>`
//! entries that map a day percentage to a sun colour.

use std::fs;
use std::io::{Read, Seek};
use std::path::Path;

use bevy::ecs::resource::Resource;
use thiserror::Error;

use super::{AssetError, read_archive_entry};

/// Colour used when the table has no data for the requested season.
const DEFAULT_COLOR: [f32; 3] = [1.0; 3];

/// The seasons and sun colours defined by `dayLight.xml`.
#[derive(Debug, Clone, Default, Resource)]
pub struct DayLightTable {
    seasons: Vec<Season>,
}

#[derive(Debug, Clone)]
struct Season {
    id: u8,
    hours: Vec<Hour>,
}

#[derive(Debug, Clone, Copy)]
struct Hour {
    day_percentage: u8,
    color: [f32; 3],
}

#[derive(Debug, Error)]
pub enum DayLightError {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("archive error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("asset error: {0}")]
    Asset(#[from] AssetError),
    #[error("invalid dayLight.xml: {0}")]
    Parse(String),
}

impl DayLightTable {
    /// Loads the definitions the client uses: the loose override when present,
    /// otherwise the `dayLight.xml` entry of the game's data archive.
    ///
    /// The archive is the one declared by `defaultDayLightFile`
    /// (`contents/data/data.jar`), which is a different jar from the map data
    /// holding `elements.lib`. Passing `None` is allowed so a partial install
    /// still starts with the default lighting.
    ///
    /// Missing data is not an error; the returned table is simply empty.
    pub fn load<R: Read + Seek>(
        archive: Option<&mut zip::ZipArchive<R>>,
        loose_path: Option<&Path>,
    ) -> Result<Self, DayLightError> {
        if let Some(path) = loose_path
            && path.is_file()
        {
            return Self::parse(&fs::read_to_string(path)?);
        }

        let Some(archive) = archive else {
            return Ok(Self::default());
        };
        let Some(bytes) = read_archive_entry(archive, "dayLight.xml")? else {
            return Ok(Self::default());
        };
        let xml = String::from_utf8(bytes)
            .map_err(|err| DayLightError::Parse(format!("dayLight.xml is not UTF-8: {err}")))?;
        Self::parse(&xml)
    }

    /// Parses a `dayLight.xml` document.
    pub fn parse(xml: &str) -> Result<Self, DayLightError> {
        let mut seasons: Vec<Season> = Vec::new();
        let mut current: Option<Season> = None;

        for tag in tags(xml) {
            if tag.closing {
                if tag.name == "season" {
                    finish_season(&mut current, &mut seasons);
                }
                continue;
            }
            match tag.name {
                "season" => {
                    finish_season(&mut current, &mut seasons);
                    let id = attribute(tag.rest, "id")
                        .and_then(|value| value.parse::<u8>().ok())
                        .ok_or_else(|| {
                            DayLightError::Parse("season without a numeric id".into())
                        })?;
                    current = Some(Season {
                        id,
                        hours: Vec::new(),
                    });
                }
                "hour" => {
                    if let Some(season) = current.as_mut() {
                        season.hours.push(parse_hour(tag.rest)?);
                    }
                }
                _ => {}
            }
        }
        finish_season(&mut current, &mut seasons);

        if seasons.is_empty() {
            return Err(DayLightError::Parse("no <season> entries".into()));
        }
        seasons.sort_by_key(|season| season.id);
        seasons.dedup_by_key(|season| season.id);

        Ok(Self { seasons })
    }

    /// Returns `true` when no definitions were loaded.
    pub fn is_empty(&self) -> bool {
        self.seasons.is_empty()
    }

    /// Returns the season ids present in the table, in ascending order.
    pub fn season_ids(&self) -> impl ExactSizeIterator<Item = u8> + '_ {
        self.seasons.iter().map(|season| season.id)
    }

    /// Returns the sun colour for `season` at `day_percentage`.
    ///
    /// Mirrors `SunLightModifier.setDayTime`: the two `<hour>` entries around the
    /// percentage are interpolated, wrapping across the end of the season.
    pub fn sun_color(&self, season: u8, day_percentage: f32) -> [f32; 3] {
        let Some(definition) = self.seasons.iter().find(|entry| entry.id == season) else {
            return DEFAULT_COLOR;
        };
        let hours = definition.hours.as_slice();
        match hours {
            [] => DEFAULT_COLOR,
            [hour] => hour.color,
            _ => {
                let percentage = day_percentage.clamp(0.0, 100.0);
                // `DayLightDefinitionManager.get` returns the last definition at
                // or before the percentage; `getNext` follows it, wrapping.
                let next_index =
                    hours.partition_point(|hour| f32::from(hour.day_percentage) <= percentage);
                let prev = hours[(next_index + hours.len() - 1) % hours.len()];
                let next = hours[next_index % hours.len()];

                let from = f32::from(prev.day_percentage);
                let mut to = f32::from(next.day_percentage);
                let mut percentage = percentage;
                if percentage < from {
                    percentage += 100.0;
                }
                if percentage > to {
                    to += 100.0;
                }
                let amount = if to > from {
                    (percentage - from) / (to - from)
                } else {
                    0.0
                };

                std::array::from_fn(|channel| {
                    prev.color[channel] + (next.color[channel] - prev.color[channel]) * amount
                })
            }
        }
    }
}

fn finish_season(current: &mut Option<Season>, seasons: &mut Vec<Season>) {
    if let Some(mut season) = current.take() {
        season.hours.sort_by_key(|hour| hour.day_percentage);
        season.hours.dedup_by_key(|hour| hour.day_percentage);
        seasons.push(season);
    }
}

fn parse_hour(rest: &str) -> Result<Hour, DayLightError> {
    let number = |name: &str| -> Result<f32, DayLightError> {
        let raw = attribute(rest, name)
            .ok_or_else(|| DayLightError::Parse(format!("<hour> is missing '{name}'")))?;
        raw.trim()
            .parse::<f32>()
            .map_err(|_| DayLightError::Parse(format!("invalid '{name}' value '{raw}'")))
    };

    Ok(Hour {
        day_percentage: number("dayPercentage")?.clamp(0.0, 100.0) as u8,
        color: [
            (number("red")? / 255.0).clamp(0.0, 1.0),
            (number("green")? / 255.0).clamp(0.0, 1.0),
            (number("blue")? / 255.0).clamp(0.0, 1.0),
        ],
    })
}

/// A start or end tag of the simple XML dialect `dayLight.xml` uses.
struct Tag<'a> {
    name: &'a str,
    /// Raw attribute text after the tag name.
    rest: &'a str,
    closing: bool,
}

/// Iterates over the elements of a document, skipping declarations, doctypes
/// and comments. Text nodes are irrelevant here, so they are dropped.
fn tags(xml: &str) -> Vec<Tag<'_>> {
    let mut tags = Vec::new();
    let mut cursor = 0;
    while let Some(offset) = xml[cursor..].find('<') {
        let start = cursor + offset;
        let rest = &xml[start..];

        if let Some(end) = rest.strip_prefix("<!--").and_then(|r| r.find("-->")) {
            cursor = start + "<!--".len() + end + "-->".len();
            continue;
        }
        if let Some(end) = rest.strip_prefix("<?").and_then(|r| r.find("?>")) {
            cursor = start + "<?".len() + end + "?>".len();
            continue;
        }
        if let Some(end) = rest.strip_prefix("<!").and_then(|r| r.find('>')) {
            cursor = start + "<!".len() + end + 1;
            continue;
        }

        let Some(end) = xml[start..].find('>') else {
            break;
        };
        let inner = xml[start + 1..start + end].trim();
        cursor = start + end + 1;
        if inner.is_empty() {
            continue;
        }

        let (closing, inner) = match inner.strip_prefix('/') {
            Some(inner) => (true, inner.trim_start()),
            None => (false, inner),
        };
        // Self-closing separators carry no data of their own.
        let inner = inner.strip_suffix('/').unwrap_or(inner).trim_end();
        let name_end = inner
            .find(|c: char| c.is_whitespace())
            .unwrap_or(inner.len());
        tags.push(Tag {
            name: &inner[..name_end],
            rest: &inner[name_end..],
            closing,
        });
    }
    tags
}

/// Returns the value of `name` in a raw attribute string, accepting single or
/// double quotes.
fn attribute<'a>(rest: &'a str, name: &str) -> Option<&'a str> {
    let mut rest = rest;
    loop {
        let trimmed = rest.trim_start();
        let key_end = trimmed
            .find(|c: char| c == '=' || c.is_whitespace())
            .unwrap_or(trimmed.len());
        let key = &trimmed[..key_end];
        if key.is_empty() {
            return None;
        }
        rest = &trimmed[key_end..];

        let trimmed = rest.trim_start();
        let Some(value) = trimmed.strip_prefix('=') else {
            // Valueless attribute: skip it and keep looking.
            rest = trimmed;
            continue;
        };
        let value = value.trim_start();
        let quote = value.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        let value = &value[quote.len_utf8()..];
        let end = value.find(quote)?;
        if key == name {
            return Some(&value[..end]);
        }
        rest = &value[end + quote.len_utf8()..];
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Write;
    use std::path::{Path, PathBuf};

    use super::*;

    const DAY_LIGHT: &str = r#"
        <?xml version="1.0" encoding="utf-8"?>
        <!-- one entry per season -->
        <root>
            <season id="0">
                <hour dayPercentage="0" red="0" green="0" blue="0"/>
                <hour dayPercentage="50" red="255" green="128" blue="64"/>
                <hour dayPercentage="100" red="0" green="0" blue="0"/>
            </season>
            <season id='1'>
                <hour dayPercentage='20' red='255' green='0' blue='0'></hour>
                <hour dayPercentage='80' red='0' green='0' blue='255'></hour>
            </season>
        </root>
    "#;

    #[test]
    fn parses_seasons_and_sorts_hours() {
        let table = DayLightTable::parse(DAY_LIGHT).unwrap();
        assert!(!table.is_empty());
        assert_eq!(table.season_ids().collect::<Vec<_>>(), vec![0, 1]);

        let season = &table.seasons[0];
        assert_eq!(
            season
                .hours
                .iter()
                .map(|hour| hour.day_percentage)
                .collect::<Vec<_>>(),
            vec![0, 50, 100]
        );
    }

    #[test]
    fn interpolates_between_the_surrounding_hours() {
        let table = DayLightTable::parse(DAY_LIGHT).unwrap();

        assert_eq!(table.sun_color(0, 0.0), [0.0, 0.0, 0.0]);
        assert_eq!(table.sun_color(0, 50.0), [1.0, 128.0 / 255.0, 64.0 / 255.0]);
        // Halfway between 50 and 100.
        let mid = table.sun_color(0, 75.0);
        assert!((mid[0] - 0.5).abs() < 1e-6);
        assert!((mid[1] - 128.0 / 255.0 / 2.0).abs() < 1e-6);
    }

    #[test]
    fn wraps_across_the_end_of_a_season() {
        let table = DayLightTable::parse(DAY_LIGHT).unwrap();

        // 90 sits a quarter of the way from 80 back around to 20 (+100).
        let color = table.sun_color(1, 90.0);
        assert!((color[0] - 0.25).abs() < 1e-6);
        assert_eq!(color[2], 0.75);
        // Before the first entry wraps to the last one: three quarters of the
        // way from 80 back around to 20.
        let wrapped = table.sun_color(1, 10.0);
        assert_eq!(wrapped, [0.75, 0.0, 0.25]);
    }

    #[test]
    fn unknown_season_falls_back_to_white() {
        let table = DayLightTable::parse(DAY_LIGHT).unwrap();
        assert_eq!(table.sun_color(9, 50.0), [1.0, 1.0, 1.0]);
    }

    #[test]
    fn rejects_malformed_documents() {
        assert!(DayLightTable::parse("<root></root>").is_err());
        assert!(DayLightTable::parse("<season id=\"0\"><hour red=\"1\"/></season>").is_err());
        assert!(DayLightTable::parse("<season id=\"zero\"></season>").is_err());
    }

    fn temporary_archive(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "vakfu-daylight-{name}-{}-{unique}.jar",
            std::process::id()
        ))
    }

    fn write_jar(path: &Path, entries: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, bytes) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn loads_the_embedded_copy_and_prefers_the_loose_override() {
        let jar = temporary_archive("override");
        write_jar(&jar, &[("dayLight.xml", DAY_LIGHT.as_bytes())]);

        let mut archive = zip::ZipArchive::new(File::open(&jar).unwrap()).unwrap();
        assert!(
            !DayLightTable::load(Some(&mut archive), None)
                .unwrap()
                .is_empty()
        );
        drop(archive);

        // A loose file wins over the archive copy.
        let loose = jar.with_file_name("dayLight.xml");
        fs::write(
            &loose,
            "<season id=\"0\"><hour dayPercentage=\"0\" red=\"10\" green=\"20\" blue=\"30\"/></season>",
        )
        .unwrap();
        let mut archive = zip::ZipArchive::new(File::open(&jar).unwrap()).unwrap();
        let table = DayLightTable::load(Some(&mut archive), Some(&loose)).unwrap();
        assert_eq!(
            table.sun_color(0, 0.0),
            [10.0 / 255.0, 20.0 / 255.0, 30.0 / 255.0]
        );

        std::fs::remove_file(jar).unwrap();
        std::fs::remove_file(loose).unwrap();
    }

    #[test]
    fn missing_entry_or_archive_is_an_empty_table() {
        let jar = temporary_archive("missing");
        write_jar(&jar, &[("other", b"")]);

        let mut archive = zip::ZipArchive::new(File::open(&jar).unwrap()).unwrap();
        assert!(
            DayLightTable::load(Some(&mut archive), None)
                .unwrap()
                .is_empty()
        );
        std::fs::remove_file(jar).unwrap();

        // A partial install without `contents/data/data.jar` still starts.
        let archive: Option<&mut zip::ZipArchive<std::io::Cursor<Vec<u8>>>> = None;
        assert!(DayLightTable::load(archive, None).unwrap().is_empty());
    }
}
