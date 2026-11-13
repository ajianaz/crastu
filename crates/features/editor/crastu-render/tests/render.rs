//! Behaviour of `render_at` through the public API only.

use crastu_render::motion::{Curve, Easing};
use crastu_render::{
    COMPOSITION_VERSION, CaptionLayer, CaptionStyle, Composition, DisplayList, FixedAdvance,
    Highlight, Karaoke, Layer, LayerKind, PopIn, Primitive, Rect, Rgba, SoftFrame, TitleLayer,
    WordTiming, frame_time, render_at,
};

const WHITE: Rgba = Rgba::rgb(255, 255, 255);
const RED: Rgba = Rgba::rgb(255, 0, 0);
const GOLD: Rgba = Rgba::rgb(255, 200, 0);

/// One character is 10 px wide and 10 px high, so "aa bb cc" is 80 px: x 460..540 in a 1000 px
/// frame, with "aa" at 460, "bb" at 490 and "cc" at 520; y is 95 in a 200 px frame.
fn layout() -> FixedAdvance {
    FixedAdvance {
        advance: 1.0,
        line_height: 1.0,
    }
}

fn words() -> Vec<WordTiming> {
    vec![
        WordTiming::new("aa", 1.0, 1.5),
        WordTiming::new("bb", 1.5, 2.0),
        WordTiming::new("cc", 2.5, 3.0),
    ]
}

fn style() -> CaptionStyle {
    CaptionStyle {
        font_size: 10.0,
        color: WHITE,
        area: Rect::new(0.0, 0.0, 1.0, 1.0),
        words_per_page: 3,
        linger: 0.3,
        ..CaptionStyle::default()
    }
}

fn comp_with(style: CaptionStyle) -> Composition {
    Composition {
        version: COMPOSITION_VERSION,
        width: 1000,
        height: 200,
        fps: 30,
        duration: 5.0,
        layers: vec![
            Layer::new(
                0.0,
                5.0,
                LayerKind::Solid {
                    color: Rgba::rgb(0, 0, 40),
                },
            ),
            Layer::new(
                0.5,
                3.0,
                LayerKind::Title(TitleLayer {
                    text: "Title".into(),
                    font_size: 20.0,
                    color: WHITE,
                    area: Rect::new(0.0, 0.0, 1.0, 0.3),
                    enter: Some(PopIn {
                        from_scale: 0.5,
                        curve: Curve::Ease {
                            easing: Easing::Linear,
                            duration: 0.5,
                        },
                    }),
                }),
            ),
            Layer::new(
                0.0,
                5.0,
                LayerKind::Caption(CaptionLayer {
                    words: words(),
                    style,
                }),
            ),
        ],
    }
}

fn texts(list: &DisplayList) -> Vec<(&str, Rgba, f64, f64)> {
    list.primitives
        .iter()
        .filter_map(|p| match p {
            Primitive::Text(r) => Some((r.text.as_str(), r.color, r.scale, r.opacity)),
            _ => None,
        })
        .collect()
}

fn caption_runs(c: &Composition, t: f64) -> Vec<(String, Rgba, f64, f64)> {
    texts(&render_at(c, t, &layout()).list)
        .into_iter()
        .filter(|r| r.0 != "Title")
        .map(|(a, b, c, d)| (a.to_owned(), b, c, d))
        .collect()
}

#[test]
fn same_frame_twice_and_in_any_order_is_identical() {
    let mut s = style();
    s.highlight = Some(Highlight { color: RED });
    s.karaoke = Some(Karaoke { fill: GOLD });
    s.pop_in = Some(PopIn {
        from_scale: 0.6,
        curve: Curve::Spring {
            spring: crastu_render::motion::Spring::new(0.4, 0.4),
        },
    });
    let c = comp_with(s);
    let times: Vec<f64> = (0..150).map(|i| frame_time(&c, i)).collect();
    let forward: Vec<_> = times.iter().map(|&t| render_at(&c, t, &layout())).collect();
    let soft: Vec<_> = times
        .iter()
        .map(|&t| SoftFrame::render(&c, t, &layout()))
        .collect();
    let mut order: Vec<usize> = (0..times.len()).collect();
    order.sort_by_key(|&i| (i * 7919) % 151);
    for &i in &order {
        assert_eq!(render_at(&c, times[i], &layout()), forward[i], "frame {i}");
        assert_eq!(
            SoftFrame::render(&c, times[i], &layout()),
            soft[i],
            "frame {i}"
        );
    }
    assert_eq!(
        render_at(&c, times[40], &layout()),
        render_at(&c, times[40], &layout())
    );
}

#[test]
fn seeking_equals_playing_up_to_it() {
    let mut s = style();
    s.highlight = Some(Highlight { color: RED });
    s.karaoke = Some(Karaoke { fill: GOLD });
    let c = comp_with(s);
    let target = 77;
    // "Play": render every frame in order on one composition, then compare the last to a seek
    // on a fresh copy of the data.
    let mut last = None;
    for i in 0..=target {
        last = Some(SoftFrame::render(&c, frame_time(&c, i), &layout()));
    }
    let fresh = c.clone();
    let seeked = SoftFrame::render(&fresh, frame_time(&fresh, target), &layout());
    assert_eq!(last, Some(seeked));
}

#[test]
fn highlight_is_exactly_at_word_start() {
    let mut s = style();
    s.highlight = Some(Highlight { color: RED });
    let c = comp_with(s);
    assert!(
        caption_runs(&c, 0.999_999).is_empty(),
        "nothing before the first word"
    );
    let at_start = caption_runs(&c, 1.0);
    let colours: Vec<_> = at_start.iter().map(|r| (r.0.as_str(), r.1)).collect();
    assert_eq!(colours, [("aa", RED), ("bb", WHITE), ("cc", WHITE)]);
    // Just before the next word starts, the first is still lit; at its start the lit word moves.
    let before = caption_runs(&c, 1.499_999);
    assert_eq!((before[0].1, before[1].1), (RED, WHITE));
    let at = caption_runs(&c, 1.5);
    assert_eq!((at[0].1, at[1].1), (WHITE, RED));
    // A gap between words (2.0..2.5) lights nothing; the page is still shown.
    let gap = caption_runs(&c, 2.2);
    assert_eq!(gap.len(), 3);
    assert!(gap.iter().all(|r| r.1 == WHITE));
}

#[test]
fn unspoken_words_dim() {
    let mut s = style();
    s.unspoken_opacity = 0.25;
    let c = comp_with(s);
    let r = caption_runs(&c, 1.0);
    assert_eq!((r[0].3, r[1].3, r[2].3), (1.0, 0.25, 0.25));
}

#[test]
fn pop_in_follows_the_curve_from_the_word_start() {
    let mut s = style();
    s.pop_in = Some(PopIn {
        from_scale: 0.5,
        curve: Curve::Ease {
            easing: Easing::CubicOut,
            duration: 0.2,
        },
    });
    let c = comp_with(s);
    let r = caption_runs(&c, 1.0);
    assert_eq!(
        r[0].2, 0.5,
        "starts at from_scale exactly at the word start"
    );
    assert_eq!(r[1].2, 0.5, "a word to come waits at from_scale");
    let r = caption_runs(&c, 1.1);
    // 0.5 + 0.5 * CubicOut(0.5) = 0.5 + 0.5 * 0.875
    assert!((r[0].2 - 0.9375).abs() < 1e-12, "{}", r[0].2);
    assert_eq!(caption_runs(&c, 1.2)[0].2, 1.0);
    assert_eq!(caption_runs(&c, 1.4)[0].2, 1.0);
}

#[test]
fn karaoke_fills_left_to_right_and_stays_filled() {
    let mut s = style();
    s.karaoke = Some(Karaoke { fill: GOLD });
    let c = comp_with(s);
    let list = render_at(&c, 1.25, &layout()).list;
    let p = &list.primitives;
    let i = p
        .iter()
        .position(|p| matches!(p, Primitive::PushClip(_)))
        .expect("a clip for the word being sung");
    let Primitive::PushClip(clip) = &p[i] else {
        unreachable!()
    };
    // "aa" is at x 460, 20 px wide; halfway through its span half is filled.
    assert_eq!((clip.x, clip.w), (460.0, 10.0));
    assert!(matches!(&p[i - 1], Primitive::Text(r) if r.text == "aa" && r.color == WHITE));
    assert!(matches!(&p[i + 1], Primitive::Text(r) if r.text == "aa" && r.color == GOLD));
    assert!(matches!(&p[i + 2], Primitive::PopClip));
    // Later: aa is fully filled with no clip, bb is being sung.
    let r = caption_runs(&c, 1.75);
    assert_eq!((r[0].0.as_str(), r[0].1), ("aa", GOLD));
    // And the pixels agree: the left half of "aa" is gold, the right half white.
    let f = SoftFrame::render(&c, 1.25, &layout());
    assert_eq!(f.pixel(465, 100), Some([255, 200, 0, 255]));
    assert_eq!(f.pixel(475, 100), Some([255, 255, 255, 255]));
}

#[test]
fn pages_follow_the_words_and_linger() {
    let mut s = style();
    s.words_per_page = 2;
    let c = comp_with(s);
    let names = |t| {
        caption_runs(&c, t)
            .into_iter()
            .map(|r| r.0)
            .collect::<Vec<_>>()
    };
    assert_eq!(names(1.0), ["aa", "bb"]);
    assert_eq!(
        names(2.29),
        ["aa", "bb"],
        "page lingers 0.3 s after its last word"
    );
    assert!(names(2.3).is_empty(), "hidden between pages");
    assert_eq!(names(2.5), ["cc"]);
    assert_eq!(names(3.29), ["cc"]);
    assert!(names(3.3).is_empty());
}

#[test]
fn title_pops_in_with_opacity() {
    let c = comp_with(style());
    let title = |t| {
        texts(&render_at(&c, t, &layout()).list)
            .into_iter()
            .find(|r| r.0 == "Title")
            .map(|r| (r.2, r.3))
    };
    assert_eq!(title(0.499), None);
    assert_eq!(title(0.5), Some((0.5, 0.0)));
    let (s, o) = title(0.75).unwrap();
    assert!((s - 0.75).abs() < 1e-12 && (o - 0.5).abs() < 1e-12);
    assert_eq!(title(1.0), Some((1.0, 1.0)));
    assert_eq!(title(3.499), Some((1.0, 1.0)));
    assert_eq!(title(3.5), None);
}

#[test]
fn layers_paint_bottom_to_top_and_pixels_show_it() {
    let c = comp_with(style());
    let list = render_at(&c, 1.0, &layout()).list;
    assert!(matches!(list.primitives[0], Primitive::Rect { .. }));
    let f = SoftFrame::render(&c, 1.0, &layout());
    assert_eq!(f.pixel(0, 199), Some([0, 0, 40, 255]), "solid background");
    assert_eq!(
        f.pixel(465, 100),
        Some([255, 255, 255, 255]),
        "caption block"
    );
}

#[test]
fn bad_times_do_not_panic() {
    let c = comp_with(style());
    for t in [f64::NAN, f64::INFINITY, -5.0, 1e12] {
        let _ = render_at(&c, t, &layout());
    }
    assert_eq!(
        render_at(&c, f64::NAN, &layout()),
        render_at(&c, 0.0, &layout())
    );
}
