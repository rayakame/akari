//! Where media goes, from sizes alone, so a row's height is final before anything loads.
//! Points, with Discord's numbers (canary 649848: modules 394839, 838541, 619517, 515718 for
//! attachments, 659674 for embeds, 559296 for the forum's gallery).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// A message's visual attachments: each cell's place in a box of `size`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mosaic {
    pub size: Size,
    pub cells: Vec<Rect>,
}

/// The forum gallery's columns for a page `width` wide.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gallery {
    pub columns: u32,
    pub column_width: f64,
}

pub const MOSAIC_WIDTH: u32 = 550;
pub const MOSAIC_HEIGHT: u32 = 350;
pub const MOSAIC_GAP: u32 = 4;
pub const STICKER: Size = Size {
    width: 160,
    height: 160,
};
const TOP_HEIGHT: u32 = 280;
const PAIR: u32 = (MOSAIC_WIDTH - MOSAIC_GAP) / 2;
const VIDEO_MIN: Size = Size {
    width: 150,
    height: 110,
};
const EMBED_IMAGE: Size = Size {
    width: 400,
    height: 300,
};
// TikTok's and portrait YouTube videos (module 659674 `ds`).
const EMBED_TALL: Size = Size {
    width: 400,
    height: 450,
};
const EMBED_VIDEO_MIN: Size = Size {
    width: 150,
    height: 144,
};
const EMBED_THUMBNAIL: Size = Size {
    width: 80,
    height: 80,
};
const GALLERY_MIN: f64 = 320.0;
const GALLERY_MAX: f64 = 450.0;
const GALLERY_GAP: f64 = 16.0;

/// `source` scaled into `max`, never up.
pub fn contain(source: Size, max: Size) -> Size {
    scaled(source, scale(source, max, f64::min))
}

/// The size `source` is drawn at to fill `cell`, never up; the overflow is cropped.
pub fn cover(source: Size, cell: Size) -> Size {
    scaled(source, scale(source, cell, f64::max))
}

fn scale(source: Size, bound: Size, pick: fn(f64, f64) -> f64) -> f64 {
    if source.width == 0 || source.height == 0 {
        return 1.0;
    }
    pick(
        f64::from(bound.width) / f64::from(source.width),
        f64::from(bound.height) / f64::from(source.height),
    )
    .min(1.0)
}

fn scaled(source: Size, factor: f64) -> Size {
    let side = |length: u32| (f64::from(length) * factor).round() as u32;
    Size {
        width: side(source.width),
        height: side(source.height),
    }
}

/// One attachment alone: contained in 550 × 350; a video at least 150 × 110.
pub fn media_box(source: Size, video: bool) -> Size {
    let fitted = contain(
        source,
        Size {
            width: MOSAIC_WIDTH,
            height: MOSAIC_HEIGHT,
        },
    );
    if video {
        Size {
            width: fitted.width.max(VIDEO_MIN.width),
            height: fitted.height.max(VIDEO_MIN.height),
        }
    } else {
        fitted
    }
}

/// The cells of 1 to 10 visual attachments at Discord's 550 points: one alone; two side by
/// side; three as one large and two stacked; four as two by two; from five on a top part (one
/// wide or a pair) over rows of three squares. No more than ten are laid out.
pub fn mosaic(items: &[Size]) -> Mosaic {
    let count = items.len().min(10);
    let mut cells = Vec::with_capacity(count);
    let rect = |x, y, width, height| Rect {
        x,
        y,
        width,
        height,
    };
    match count {
        0 => {}
        1 => {
            let fitted = media_box(items[0], false);
            cells.push(rect(0, 0, fitted.width, fitted.height));
        }
        2 => {
            cells.push(rect(0, 0, PAIR, PAIR));
            cells.push(rect(PAIR + MOSAIC_GAP, 0, PAIR, PAIR));
        }
        3 => {
            let large = (MOSAIC_WIDTH - MOSAIC_GAP) * 2 / 3;
            let small = MOSAIC_WIDTH - MOSAIC_GAP - large;
            let half = (MOSAIC_HEIGHT - MOSAIC_GAP) / 2;
            cells.push(rect(0, 0, large, MOSAIC_HEIGHT));
            cells.push(rect(large + MOSAIC_GAP, 0, small, half));
            cells.push(rect(large + MOSAIC_GAP, half + MOSAIC_GAP, small, half));
        }
        4 => {
            let half = (MOSAIC_HEIGHT - MOSAIC_GAP) / 2;
            for row in 0..2 {
                for column in 0..2 {
                    cells.push(rect(
                        column * (PAIR + MOSAIC_GAP),
                        row * (half + MOSAIC_GAP),
                        PAIR,
                        half,
                    ));
                }
            }
        }
        _ => {
            let mut y = 0;
            match count % 3 {
                1 => {
                    cells.push(rect(0, 0, MOSAIC_WIDTH, TOP_HEIGHT));
                    y = TOP_HEIGHT + MOSAIC_GAP;
                }
                2 => {
                    cells.push(rect(0, 0, PAIR, PAIR));
                    cells.push(rect(PAIR + MOSAIC_GAP, 0, PAIR, PAIR));
                    y = PAIR + MOSAIC_GAP;
                }
                _ => {}
            }
            let edges = thirds();
            while cells.len() < count {
                let side = edges[1] - MOSAIC_GAP;
                for column in 0..3 {
                    let (start, end) = (edges[column], edges[column + 1]);
                    let width = end - start - MOSAIC_GAP;
                    cells.push(rect(start, y, width, side));
                }
                y += side + MOSAIC_GAP;
            }
        }
    }
    let width = cells
        .iter()
        .map(|cell| cell.x + cell.width)
        .max()
        .unwrap_or(0);
    let height = cells
        .iter()
        .map(|cell| cell.y + cell.height)
        .max()
        .unwrap_or(0);
    Mosaic {
        size: Size { width, height },
        cells,
    }
}

// Where each of three columns starts, then 550 plus a gap: each cell ends a gap before the next.
fn thirds() -> [u32; 4] {
    let cell = f64::from(MOSAIC_WIDTH - 2 * MOSAIC_GAP) / 3.0;
    let start = |column: u32| (f64::from(column) * (cell + f64::from(MOSAIC_GAP))).round() as u32;
    [start(0), start(1), start(2), MOSAIC_WIDTH + MOSAIC_GAP]
}

/// `mosaic` scaled down as a whole to fit a column `width` points wide; never up.
pub fn fit_width(mosaic: &Mosaic, width: u32) -> Mosaic {
    if mosaic.size.width <= width || mosaic.size.width == 0 {
        return mosaic.clone();
    }
    let factor = f64::from(width) / f64::from(mosaic.size.width);
    let at = |value: u32| (f64::from(value) * factor).round() as u32;
    let cells = mosaic
        .cells
        .iter()
        .map(|cell| {
            let (x, y) = (at(cell.x), at(cell.y));
            Rect {
                x,
                y,
                width: at(cell.x.saturating_add(cell.width)).saturating_sub(x),
                height: at(cell.y.saturating_add(cell.height)).saturating_sub(y),
            }
        })
        .collect();
    Mosaic {
        size: Size {
            width,
            height: at(mosaic.size.height),
        },
        cells,
    }
}

/// An embed card's image: contained in 400 × 300.
pub fn embed_image(source: Size) -> Size {
    contain(source, EMBED_IMAGE)
}

/// An embed's image, gallery or poster: contained in 400 × 300, or 400 × 450 when `tall`.
pub fn embed_media(source: Size, tall: bool) -> Size {
    contain(source, if tall { EMBED_TALL } else { EMBED_IMAGE })
}

/// An embed's video: as its media, at least 150 × 144.
pub fn embed_video(source: Size, tall: bool) -> Size {
    let fitted = embed_media(source, tall);
    Size {
        width: fitted.width.max(EMBED_VIDEO_MIN.width),
        height: fitted.height.max(EMBED_VIDEO_MIN.height),
    }
}

/// An embed card's thumbnail: contained in 80 × 80.
pub fn embed_thumbnail(source: Size) -> Size {
    contain(source, EMBED_THUMBNAIL)
}

/// The forum gallery's masonry: columns between 320 and 450 wide, 16 apart.
pub fn gallery_columns(width: f64) -> Gallery {
    let wide = width / GALLERY_MAX;
    let columns = ((wide + (width / GALLERY_MIN - wide) / 2.0).floor() as u32).max(1);
    Gallery {
        columns,
        column_width: (width - GALLERY_GAP * f64::from(columns - 1)) / f64::from(columns),
    }
}

/// A gallery card's height for its column's width: its header, the media at `ratio` (height
/// over width: 2/3 in forums, 9/16 in media channels), the tags' row and its footer.
pub fn gallery_card_height(column_width: f64, ratio: f64) -> f64 {
    60.0 + ((column_width - 26.0) * ratio).ceil() + 24.0 + 36.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size(width: u32, height: u32) -> Size {
        Size { width, height }
    }

    #[test]
    fn contain_never_upscales() {
        let max = size(550, 350);
        for (source, fitted) in [
            (size(1920, 1080), size(550, 309)),
            (size(1080, 1920), size(197, 350)),
            (size(100, 5000), size(7, 350)),
            (size(200, 100), size(200, 100)),
            (size(550, 350), size(550, 350)),
        ] {
            assert_eq!(contain(source, max), fitted, "{source:?}");
        }
    }

    #[test]
    fn cover_fills_the_cell_without_upscaling() {
        assert_eq!(cover(size(1920, 1080), size(273, 273)), size(485, 273));
        assert_eq!(cover(size(1080, 1920), size(273, 173)), size(273, 485));
        assert_eq!(cover(size(100, 50), size(273, 273)), size(100, 50));
    }

    #[test]
    fn a_video_box_is_at_least_150_by_110() {
        assert_eq!(media_box(size(100, 50), true), size(150, 110));
        assert_eq!(media_box(size(100, 50), false), size(100, 50));
        assert_eq!(media_box(size(1920, 1080), true), size(550, 309));
    }

    #[test]
    fn mosaics_follow_discords_cells() {
        let item = size(1000, 1000);
        assert_eq!(
            mosaic(&[item]).cells,
            vec![Rect {
                x: 0,
                y: 0,
                width: 350,
                height: 350
            }]
        );
        assert_eq!(
            mosaic(&[item; 3]).cells,
            vec![
                Rect {
                    x: 0,
                    y: 0,
                    width: 364,
                    height: 350
                },
                Rect {
                    x: 368,
                    y: 0,
                    width: 182,
                    height: 173
                },
                Rect {
                    x: 368,
                    y: 177,
                    width: 182,
                    height: 173
                },
            ]
        );
        assert_eq!(
            mosaic(&[item; 4]).cells,
            vec![
                Rect {
                    x: 0,
                    y: 0,
                    width: 273,
                    height: 173
                },
                Rect {
                    x: 277,
                    y: 0,
                    width: 273,
                    height: 173
                },
                Rect {
                    x: 0,
                    y: 177,
                    width: 273,
                    height: 173
                },
                Rect {
                    x: 277,
                    y: 177,
                    width: 273,
                    height: 173
                },
            ]
        );
        assert_eq!(mosaic(&[item; 2]).size, size(550, 273));
    }

    #[test]
    fn larger_mosaics_have_a_top_and_rows_of_three() {
        for (count, top, rows) in [
            (5, 2, 1),
            (6, 0, 2),
            (7, 1, 2),
            (8, 2, 2),
            (9, 0, 3),
            (10, 1, 3),
        ] {
            let laid = mosaic(&vec![size(800, 600); count]);
            assert_eq!(laid.cells.len(), count, "{count}");
            assert_eq!(laid.size.width, 550, "{count}");
            let squares = &laid.cells[top..];
            assert_eq!(squares.len(), rows * 3, "{count}");
            for square in squares {
                assert!(
                    square.width.abs_diff(square.height) <= 1,
                    "{count}: {square:?}"
                );
                assert!((180..=181).contains(&square.width), "{count}: {square:?}");
            }
            assert_no_overlap(&laid);
        }
        assert_eq!(mosaic(&[size(10, 10); 12]).cells.len(), 10);
        assert_eq!(mosaic(&[]).size, size(0, 0));
    }

    fn assert_no_overlap(laid: &Mosaic) {
        for (index, a) in laid.cells.iter().enumerate() {
            assert!(a.x + a.width <= laid.size.width && a.y + a.height <= laid.size.height);
            for b in &laid.cells[index + 1..] {
                let apart = a.x + a.width + MOSAIC_GAP <= b.x + 1
                    || b.x + b.width + MOSAIC_GAP <= a.x + 1
                    || a.y + a.height + MOSAIC_GAP <= b.y + 1
                    || b.y + b.height + MOSAIC_GAP <= a.y + 1;
                assert!(apart, "{a:?} and {b:?}");
            }
        }
    }

    #[test]
    fn a_narrow_column_scales_the_whole_mosaic() {
        let laid = mosaic(&[size(1000, 1000); 4]);

        let narrow = fit_width(&laid, 275);

        assert_eq!(narrow.size, size(275, 175));
        assert_eq!(
            narrow.cells[1],
            Rect {
                x: 139,
                y: 0,
                width: 136,
                height: 87
            }
        );
        assert_eq!(fit_width(&laid, 600), laid);
    }

    #[test]
    fn fitting_a_hosts_odd_mosaic_never_overflows() {
        let odd = Mosaic {
            size: size(u32::MAX, u32::MAX),
            cells: vec![Rect {
                x: u32::MAX,
                y: u32::MAX,
                width: u32::MAX,
                height: 7,
            }],
        };

        let fitted = fit_width(&odd, 100);

        assert_eq!(fitted.size.width, 100);
        assert!(fitted.cells[0].x <= 100);
    }

    #[test]
    fn embed_media_boxes() {
        assert_eq!(embed_media(size(1080, 1920), false), size(169, 300));
        assert_eq!(embed_media(size(1080, 1920), true), size(253, 450));
        assert_eq!(embed_video(size(1280, 720), false), size(400, 225));
        assert_eq!(
            embed_video(size(100, 50), false),
            size(150, 144),
            "at least 150 × 144"
        );
        assert_eq!(embed_image(size(1200, 630)), size(400, 210));
        assert_eq!(embed_image(size(300, 200)), size(300, 200));
        assert_eq!(embed_thumbnail(size(400, 400)), size(80, 80));
        assert_eq!(embed_thumbnail(size(200, 100)), size(80, 40));
    }

    #[test]
    fn gallery_columns_follow_the_formula() {
        for (width, columns, column_width) in [
            (320.0, 1, 320.0),
            (700.0, 1, 700.0),
            (1000.0, 2, 492.0),
            (1400.0, 3, 456.0),
        ] {
            let gallery = gallery_columns(width);
            assert_eq!(gallery.columns, columns, "{width}");
            assert!(
                (gallery.column_width - column_width).abs() < 1e-9,
                "{width}"
            );
        }
        assert_eq!(gallery_card_height(456.0, 2.0 / 3.0), 407.0);
        assert_eq!(gallery_card_height(456.0, 9.0 / 16.0), 362.0);
    }
}
