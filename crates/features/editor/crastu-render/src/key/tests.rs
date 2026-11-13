use super::*;
use crate::geom::{Rect, Rgba};
use crate::model::{CaptionLayer, Layer, LayerKind, TitleLayer};
use crate::style::CaptionStyle;
use crate::text::{FixedAdvance, WordTiming};

fn base() -> Composition {
    Composition {
        width: 320,
        height: 240,
        fps: 30,
        duration: 10.0,
        layers: vec![
            Layer::new(
                0.0,
                10.0,
                LayerKind::Solid {
                    color: Rgba::rgb(10, 20, 30),
                },
            ),
            Layer::new(
                1.0,
                4.0,
                LayerKind::Title(TitleLayer {
                    text: "Hello".into(),
                    font_size: 30.0,
                    color: Rgba::rgb(255, 255, 255),
                    area: Rect::new(0.1, 0.1, 0.8, 0.2),
                    enter: None,
                }),
            ),
            Layer::new(
                2.0,
                4.0,
                LayerKind::Caption(CaptionLayer {
                    words: vec![
                        WordTiming::new("a", 2.0, 2.5),
                        WordTiming::new("b", 2.5, 3.0),
                    ],
                    style: CaptionStyle::default(),
                }),
            ),
            // Not active at t = 3.0.
            Layer::new(
                8.0,
                1.0,
                LayerKind::Solid {
                    color: Rgba::rgb(1, 1, 1),
                },
            ),
        ],
    }
}

type Mutation = Box<dyn Fn(&mut Composition)>;

fn key(c: &Composition, t: f64) -> FrameKey {
    frame_key(c, t, &FixedAdvance::default())
}

#[test]
fn identical_inputs_hit() {
    assert_eq!(key(&base(), 3.0), key(&base().clone(), 3.0));
    assert_eq!(key(&base(), 0.0), key(&base(), -0.0));
    assert_eq!(key(&base(), 3.0).to_hex().len(), 64);
}

#[test]
fn every_input_of_an_active_layer_misses() {
    let k = key(&base(), 3.0);
    let mutations: Vec<(&str, Mutation)> = vec![
        (
            "solid colour",
            Box::new(|c| {
                if let LayerKind::Solid { color } = &mut c.layers[0].kind {
                    color.b ^= 1;
                }
            }),
        ),
        ("layer start", Box::new(|c| c.layers[0].start = 0.5)),
        ("layer opacity", Box::new(|c| c.layers[0].opacity = 0.5)),
        (
            "title text",
            Box::new(|c| {
                if let LayerKind::Title(t) = &mut c.layers[1].kind {
                    t.text.push('!');
                }
            }),
        ),
        (
            "title size",
            Box::new(|c| {
                if let LayerKind::Title(t) = &mut c.layers[1].kind {
                    t.font_size += 1.0;
                }
            }),
        ),
        (
            "word text",
            Box::new(|c| {
                if let LayerKind::Caption(l) = &mut c.layers[2].kind {
                    l.words[1].text = "c".into();
                }
            }),
        ),
        (
            "word end",
            Box::new(|c| {
                if let LayerKind::Caption(l) = &mut c.layers[2].kind {
                    l.words[0].end += 0.001;
                }
            }),
        ),
        (
            "caption style",
            Box::new(|c| {
                if let LayerKind::Caption(l) = &mut c.layers[2].kind {
                    l.style.font_size += 1.0;
                }
            }),
        ),
        ("layer order", Box::new(|c| c.layers.swap(1, 2))),
        ("frame width", Box::new(|c| c.width += 1)),
        ("frame height", Box::new(|c| c.height += 1)),
    ];
    for (name, m) in mutations {
        let mut c = base();
        m(&mut c);
        assert_ne!(key(&c, 3.0), k, "{name} must change the key");
    }
}

#[test]
fn time_layout_and_active_set_change_the_key() {
    let k = key(&base(), 3.0);
    assert_ne!(key(&base(), 3.0 + 1e-9), k);
    let other = FixedAdvance {
        advance: 0.6,
        ..FixedAdvance::default()
    };
    assert_ne!(frame_key(&base(), 3.0, &other), k);
    // The set of active layers differs at another time.
    assert_ne!(key(&base(), 8.5), key(&base(), 7.5));
}

#[test]
fn inactive_layers_and_fps_do_not_change_the_key() {
    let k = key(&base(), 3.0);
    let mut c = base();
    if let LayerKind::Solid { color } = &mut c.layers[3].kind {
        color.r = 99;
    }
    c.fps = 60;
    c.duration = 20.0;
    assert_eq!(key(&c, 3.0), k);
}
