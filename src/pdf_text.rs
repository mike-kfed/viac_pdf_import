use std::collections::HashMap;
use std::convert::TryInto;
use std::rc::Rc;

use pdf::content::*;
use pdf::error::{PdfError, Result};
use pdf::font::*;
use pdf::object::*;
use pdf::primitive::Primitive;

use pdf::encoding::BaseEncoding;
use pdf_encoding::{self, DifferenceForwardMap};

use euclid::Transform2D;

#[derive(Clone, Default)]
enum Decoder {
    Map(DifferenceForwardMap),
    Cmap(ToUnicodeMap),
    #[default]
    None,
}

#[derive(Default, Clone)]
pub struct FontInfo {
    decoder: Decoder,
}

impl FontInfo {
    pub fn decode(&self, data: &[u8], out: &mut String) -> Result<()> {
        match &self.decoder {
            Decoder::Cmap(ref cmap) => {
                // FIXME: not sure the BOM is obligatory
                if data.starts_with(&[0xfe, 0xff]) {
                    // FIXME: really windows not chunks!?
                    for w in data.windows(2) {
                        let cp = u16::from_be_bytes(w.try_into().unwrap());
                        if let Some(s) = cmap.get(cp) {
                            out.push_str(s);
                        }
                    }
                } else {
                    out.extend(
                        data.iter()
                            .filter_map(|&b| cmap.get(b.into()).map(|v| v.to_owned())),
                    );
                }
                Ok(())
            }
            Decoder::Map(map) => {
                out.extend(
                    data.iter()
                        .filter_map(|&b| map.get(b).map(|v| v.to_owned())),
                );
                Ok(())
            }
            Decoder::None => {
                if data.starts_with(&[0xfe, 0xff]) {
                    utf16be_to_char(&data[2..]).try_for_each(|r| {
                        r.map_or(Err(PdfError::Utf16Decode), |c| {
                            out.push(c);
                            Ok(())
                        })
                    })
                } else {
                    match std::str::from_utf8(data) {
                        Ok(text) => {
                            out.push_str(text);
                            Ok(())
                        }
                        Err(e) => {
                            log::error!("err: {:?} data: {:?}", e, data);
                            /* // for debugging show ascii text
                                let ascii: Vec<u8> =
                            log::error!("ascii: {:?}", std::str::from_utf8(&ascii));
                            */
                            Err(PdfError::Utf8Decode)
                        }
                    }
                }
            }
        }
    }
}

struct FontCache<'src, T: Resolve> {
    fonts: HashMap<String, Rc<FontInfo>>,
    page: &'src Page,
    resolve: &'src T,
    default_font: Rc<FontInfo>,
}

impl<'src, T: Resolve> FontCache<'src, T> {
    fn new(page: &'src Page, resolve: &'src T) -> Self {
        let mut cache = FontCache {
            fonts: HashMap::new(),
            page,
            resolve,
            default_font: Rc::new(FontInfo::default()),
        };

        cache.populate();

        cache
    }

    fn populate(&mut self) {
        if let Ok(resources) = self.page.resources() {
            for (name, font) in resources.fonts.iter() {
                if let Ok(font) = self.resolve.get(font.as_ref().unwrap()) {
                    self.add_font(name.to_string(), font);
                }
            }

            for (font, _) in resources.graphics_states.values().filter_map(|gs| gs.font) {
                if let Ok(font) = self.resolve.get(font) {
                    if let Some(name) = &font.name {
                        self.add_font(name.to_string(), font);
                    }
                }
            }
        }
    }

    fn add_font(&mut self, name: impl Into<String>, font: RcRef<Font>) {
        let decoder = if let Some(to_unicode) = font.to_unicode(self.resolve) {
            let cmap = to_unicode.unwrap();
            Decoder::Cmap(cmap)
        } else if let Some(encoding) = font.encoding() {
            let map = match encoding.base {
                BaseEncoding::StandardEncoding => Some(&pdf_encoding::STANDARD),
                BaseEncoding::SymbolEncoding => Some(&pdf_encoding::SYMBOL),
                BaseEncoding::WinAnsiEncoding => Some(&pdf_encoding::WINANSI),
                BaseEncoding::MacRomanEncoding => Some(&pdf_encoding::MACROMAN),
                BaseEncoding::None => None,
                ref e => {
                    eprintln!("unsupported pdf encoding {:?}", e);
                    return;
                }
            };

            Decoder::Map(DifferenceForwardMap::new(
                map,
                encoding
                    .differences
                    .clone()
                    .into_iter()
                    .map(|(i, s)| (i, s.to_string()))
                    .collect(),
            ))
        } else {
            log::error!("cannot handle font {:?} {:?}", name.into(), font);
            return;
        };

        self.fonts
            .insert(name.into(), Rc::new(FontInfo { decoder }));
    }

    fn get_by_font_name(&self, name: &str) -> Rc<FontInfo> {
        /* // for debugging when font is not found
        if !self.fonts.contains_key(name) {
            let keys: Vec<&String> = self.fonts.keys().collect();
            log::error!("font: {} not found in {:?}", name, keys);
        }
        */
        self.fonts.get(name).unwrap_or(&self.default_font).clone()
    }

    fn get_by_graphic_state_name(&self, name: &str) -> Option<(Rc<FontInfo>, f32)> {
        self.page
            .resources()
            .ok()
            .and_then(|resources| resources.graphics_states.get(name))
            .and_then(|gs| gs.font)
            .map(|(font, font_size)| {
                let font = self
                    .resolve
                    .get(font)
                    .ok()
                    .map(|font| {
                        let name = format!("{}", font.name.as_ref().unwrap());
                        self.get_by_font_name(&name)
                    })
                    .unwrap_or_else(|| self.default_font.clone());
                (font, font_size)
            })
    }
}

#[derive(Clone, Default)]
pub struct TextState {
    pub font: Rc<FontInfo>,
    pub font_size: f32,
    pub text_leading: f32,
    pub text_matrix: Transform2D<f32, PdfSpace, PdfSpace>,
}

pub fn ops_with_text_state<'src, T: Resolve>(
    page: &'src Page,
    resolve: &'src T,
) -> impl Iterator<Item = (Op, Rc<TextState>)> + 'src {
    page.contents.iter().flat_map(move |contents| {
        contents.operations(resolve).unwrap().into_iter().scan(
            (Rc::new(TextState::default()), FontCache::new(page, resolve)),
            |(state, font_cache), op| {
                let mut update_state = |update_fn: &dyn Fn(&mut TextState)| {
                    let old_state: &TextState = state;
                    let mut new_state = old_state.clone();

                    update_fn(&mut new_state);

                    *state = Rc::new(new_state);
                };

                match op {
                    Op::BeginText => {
                        /* do nothing, to keep TextState for decoding */
                        // *state = Default::default();
                    }
                    Op::GraphicsState { ref name } => {
                        update_state(&|state: &mut TextState| {
                            if let Some((font, font_size)) =
                                font_cache.get_by_graphic_state_name(name)
                            {
                                state.font = font;
                                state.font_size = font_size;
                            }
                        });
                    }
                    Op::TextFont { ref name, size } => {
                        update_state(&|state: &mut TextState| {
                            state.font = font_cache.get_by_font_name(&format!("{name}"));
                            state.font_size = size;
                        });
                    }
                    Op::Leading { leading } => {
                        update_state(&|state: &mut TextState| state.text_leading = leading);
                    }
                    Op::TextNewline => {
                        update_state(&|state: &mut TextState| {
                            state.text_matrix = state.text_matrix.pre_translate(
                                Point {
                                    x: 0.0f32,
                                    y: state.text_leading,
                                }
                                .into(),
                            );
                        });
                    }
                    Op::MoveTextPosition { translation } => {
                        update_state(&|state: &mut TextState| {
                            state.text_matrix = state.text_matrix.pre_translate(translation.into());
                        });
                    }
                    Op::SetTextMatrix { matrix } => {
                        update_state(&|state: &mut TextState| {
                            state.text_matrix = matrix.into();
                        });
                    }
                    _ => {}
                }

                Some((op, state.clone()))
            },
        )
    })
}

pub fn page_text(page: &Page, resolve: &impl Resolve) -> Result<String, PdfError> {
    let mut out = String::new();

    for (op, text_state) in ops_with_text_state(page, resolve) {
        match op {
            Op::TextDraw { ref text } => text_state.font.decode(&text.data, &mut out)?,
            Op::TextDrawAdjusted { ref array } => {
                for data in array {
                    if let TextDrawAdjusted::Text(text) = data {
                        text_state.font.decode(&text.data, &mut out)?;
                    }
                }
            }
            Op::TextNewline => {
                out.push('\n');
            }
            Op::MoveTextPosition { translation } => {
                if translation.y.abs() < f32::EPSILON {
                    out.push('\n');
                } else if translation.x.abs() > 3. {
                    out.push('\t');
                }
            }
            Op::SetTextMatrix { matrix } => {
                if (matrix.f - text_state.text_matrix.m32).abs() < f32::EPSILON {
                    out.push('\n');
                } else {
                    out.push('\t');
                }
            }
            Op::BeginMarkedContent {
                ref tag,
                ref properties,
            } => {
                // TODO: handle all tag, and props enum variants with text?
                if tag == "Span" {
                    if let Some(props) = properties {
                        match props {
                            Primitive::Dictionary(d) => {
                                if let Some(Primitive::String(text)) = d.get("ActualText") {
                                    text_state.font.decode(&text.data, &mut out)?;
                                }
                            }
                            x => {
                                return Err(PdfError::UnexpectedPrimitive {
                                    expected: "Dictionary",
                                    found: x.get_debug_name(),
                                })
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

pub(crate) fn pdf2strings<B: pdf::backend::Backend>(
    file: pdf::file::CachedFile<B>,
) -> Result<Vec<String>, PdfError> {
    let mut all_pages = vec![];
    for page in file.pages().flatten() {
        all_pages.push(page_text(&page, &file)?);
    }
    Ok(all_pages)
}
