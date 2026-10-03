use prpr::{
    core::{
        block_area::{block_ease, blocks_touch, BlockArea, BlockPhase, BlockPoint, BlockedFingers},
        ChartExtra,
    },
    parse::parse_phigros,
};
use serde_json::json;

fn area() -> BlockArea {
    serde_json::from_value(json!({
        "topRightPercentage": {"x": 0.75, "y": 0.75},
        "bottomLeftPercentage": {"x": 0.25, "y": 0.25},
        "appearTime": 1.0, "enableTime": 2.0, "disableTime": 4.0, "disappearTime": 5.0
    }))
    .unwrap()
}

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.00001, "{actual} != {expected}");
}

#[test]
fn phases_are_half_open_seconds() {
    let a = area();
    for (t, phase) in [
        (0.999, BlockPhase::Hidden),
        (1., BlockPhase::Disabled),
        (1.5, BlockPhase::Ready),
        (2., BlockPhase::Active),
        (4., BlockPhase::Disabled),
        (5., BlockPhase::Hidden),
    ] {
        assert_eq!(a.phase(t), phase);
    }
    close(a.opacity(1.25), 0.5);
    close(a.opacity(4.999), 1.);
    close(a.opacity(5.), 0.);
}

#[test]
fn appearing_active_skips_fade() {
    let mut a = area();
    a.appear_time = 2.1;
    assert_eq!(a.phase(2.), BlockPhase::Hidden);
    close(a.opacity(2.1), 1.);
    a.appear_time = 1.75;
    assert_eq!(a.phase(1.75), BlockPhase::Ready);
    close(a.opacity(1.75), 0.);
    close(a.opacity(2.), 1.);
}

#[test]
fn ease_uses_power_table_instead_of_rpe_ids() {
    close(block_ease(0.5, 1), 0.25);
    close(block_ease(0.5, 4), 0.125);
    close(block_ease(0.5, 10), 0.03125);
    close(block_ease(0.125, 1), (0.12_f32.powi(2) + 0.13_f32.powi(2)) / 2.);
    for kind in 0..=14 {
        close(block_ease(-2., kind), if kind == 14 { 1. } else { 0. });
        close(block_ease(2., kind), if kind == 13 { 0. } else { 1. });
    }
}

#[test]
fn first_key_defaults_then_start_key_controls_segment() {
    let mut a = area();
    a.move_events = serde_json::from_value(json!([
        {"time":2,"endPosition":{"x":0.5,"y":0.5},"easeTypeX":1,"easeTypeY":13},
        {"time":4,"endPosition":{"x":1.5,"y":1.5},"easeTypeX":14,"easeTypeY":14}
    ]))
    .unwrap();
    close(a.geometry(1., 1.).center.x, 0.);
    let at_three = a.geometry(3., 1.);
    close(at_three.center.x, 0.5);
    close(at_three.center.y, 0.);
    close(a.geometry(4., 1.).center.y, 2.);
    // Seeking backwards must not leave stale animation cursors.
    close(a.geometry(2., 1.).center.x, 0.);
    close(a.geometry(3., 1.).center.x, at_three.center.x);
}

#[test]
fn anchors_accumulate_then_movement_is_added() {
    let mut a = area();
    a.scale_events = serde_json::from_value(json!([
        {"time":0,"scale":{"x":1,"y":1},"anchor":{"x":0,"y":0.5},"easeTypeX":0,"easeTypeY":0},
        {"time":1,"scale":{"x":2,"y":1},"anchor":{"x":1,"y":0.5},"easeTypeX":0,"easeTypeY":0},
        {"time":2,"scale":{"x":4,"y":1},"anchor":{"x":0.5,"y":0.5},"easeTypeX":0,"easeTypeY":0}
    ]))
    .unwrap();
    let g = a.geometry(2., 1.);
    close(g.center.x, 1.); // First anchor moves center to 1; second leaves it there.
    close(g.size.x, 4.);
    a.rotate_events = serde_json::from_value(json!([
        {"time":0,"rotation":0,"anchor":{"x":0.5,"y":0.5},"easeType":0},
        {"time":2,"rotation":90,"anchor":{"x":0.5,"y":0.5},"easeType":0}
    ]))
    .unwrap();
    close(a.geometry(2., 1.).center.y, 1.);
    a.move_events = serde_json::from_value(json!([
        {"time":0,"endPosition":{"x":0.6,"y":0.5},"easeTypeX":0,"easeTypeY":0}
    ]))
    .unwrap();
    close(a.geometry(2., 1.).center.x, 0.2);
}

#[test]
fn rotation_uses_screen_aspect_in_world_space() {
    let mut a = area();
    a.rotate_events = serde_json::from_value(json!([
        {"time":0,"rotation":90,"anchor":{"x":0.5,"y":0.5},"easeType":0}
    ]))
    .unwrap();
    let geometry = a.geometry(3., 2.);
    assert!(geometry.contains(BlockPoint { x: 0., y: 0.49 }, 0., false));
    assert!(!geometry.contains(BlockPoint { x: 0.3, y: 0. }, 0., false));
}

#[test]
fn reversed_corners_outside_screen_and_zero_scale() {
    let mut a = area();
    a.top_right_percentage = BlockPoint { x: -0.5, y: -0.2 };
    a.bottom_left_percentage = BlockPoint { x: 1.5, y: 1.2 };
    a.validate().unwrap();
    assert!(blocks_touch(std::slice::from_ref(&a), 3., 1., BlockPoint::default()));
    a.scale_events = serde_json::from_value(json!([
        {"time":0,"scale":{"x":0,"y":1},"anchor":{"x":0.5,"y":0.5},"easeTypeX":0,"easeTypeY":0}
    ]))
    .unwrap();
    assert!(!blocks_touch(&[a], 3., 1., BlockPoint::default()));
}

#[test]
fn subtract_is_parity_xor_not_simple_cutout() {
    let normal = area();
    let mut subtract = normal.clone();
    subtract.is_subtract = true;
    let p = BlockPoint::default();
    assert!(blocks_touch(&[normal.clone(), normal.clone()], 3., 1., p));
    assert!(blocks_touch(&[subtract.clone()], 3., 1., p));
    assert!(!blocks_touch(&[normal.clone(), subtract.clone()], 3., 1., p));
    assert!(!blocks_touch(&[subtract.clone(), subtract.clone()], 3., 1., p));
    assert!(blocks_touch(&[normal, subtract.clone(), subtract], 3., 1., p));
}

#[test]
fn hit_margin_is_three_percent_of_chart_height() {
    let a = area();
    assert!(blocks_touch(std::slice::from_ref(&a), 3., 1., BlockPoint { x: 0.439, y: 0. }));
    assert!(!blocks_touch(std::slice::from_ref(&a), 3., 1., BlockPoint { x: 0.441, y: 0. }));
    assert!(!blocks_touch(std::slice::from_ref(&a), 1.99, 1., BlockPoint::default()));
    assert!(!blocks_touch(&[a], 4., 1., BlockPoint::default()));
}

#[test]
fn inset_subtract_does_not_create_false_block_at_hole_boundary() {
    let mut hole = area();
    hole.is_subtract = true;
    hole.top_right_percentage = BlockPoint { x: 0.6, y: 0.6 };
    hole.bottom_left_percentage = BlockPoint { x: 0.4, y: 0.4 };
    assert!(!blocks_touch(&[area(), hole], 3., 1., BlockPoint { x: 0.21, y: 0. }));
}

#[test]
fn contact_stays_blocked_until_release_and_other_fingers_work() {
    let mut state = BlockedFingers::default();
    assert!(state.filter(1, true, false, true));
    assert!(state.filter(1, false, false, false));
    assert!(!state.filter(2, true, false, false));
    assert!(!state.filter(1, false, true, false));
    assert!(!state.filter(1, true, false, false));
    assert!(state.filter(2, false, false, true));
    state.clear();
    assert!(!state.filter(2, false, false, false));
}

#[test]
fn missing_contacts_and_reused_ids_are_released() {
    let mut state = BlockedFingers::default();
    assert!(state.filter(1, true, false, true));
    state.retain(|_| false);
    assert!(!state.filter(1, false, false, false));
    assert!(state.filter(1, true, false, true));
    assert!(!state.filter(1, true, false, false));
}

#[test]
fn parser_keeps_legacy_charts_and_reports_bad_block_index() {
    let source = json!({"formatVersion":3,"offset":0,"judgeLineList":[]});
    assert!(parse_phigros(&source.to_string(), ChartExtra::default()).unwrap().block_areas.is_empty());
    let mut source = source;
    source["blockAreaList"] = json!([{
        "topRightPercentage":{"x":1,"y":1},"bottomLeftPercentage":{"x":0,"y":0},
        "appearTime":0,"enableTime":10,"disableTime":15,"disappearTime":20,
        "rotateEvents":[{"time":0,"rotation":0,"anchor":{"x":0.5,"y":0.5},"easeType":15}]
    }]);
    let error = parse_phigros(&source.to_string(), ChartExtra::default()).err().unwrap();
    assert!(error.to_string().contains("blockAreaList[0]"));
    source["blockAreaList"][0]["rotateEvents"] = json!([]);
    let chart = parse_phigros(&source.to_string(), ChartExtra::default()).unwrap();
    assert_eq!(chart.block_areas[0].phase(10.), BlockPhase::Active);
}

#[test]
fn official_hate_in_empty_lifetime_loads_without_creating_a_block() {
    let source = json!({"formatVersion":3,"offset":0,"judgeLineList":[],"blockAreaList":[{
        "topRightPercentage":{"x":0.2402335,"y":0.89},
        "bottomLeftPercentage":{"x":0.0022451729,"y":0.0},
        "appearTime":28.3,"enableTime":28.3,"disableTime":29.3,"disappearTime":28.29932
    }]});
    let chart = parse_phigros(&source.to_string(), ChartExtra::default()).unwrap();
    let a = &chart.block_areas[0];
    assert_eq!(a.disappear_time, 28.29932);
    for t in [28., 28.29932, 28.2999, 28.3, 28.5, 29.3] {
        assert_eq!(a.phase(t), BlockPhase::Hidden);
        assert!(!blocks_touch(&chart.block_areas, t, 16. / 9., BlockPoint { x: -0.8, y: 0. }));
    }
}

#[test]
fn reversed_activation_interval_never_interferes_with_judgement() {
    let mut a = area();
    a.enable_time = 3.;
    a.disable_time = 2.;
    a.validate().unwrap();
    for t in [1.5, 2., 2.5, 3., 3.5, 4.] {
        assert_ne!(a.phase(t), BlockPhase::Active);
        assert!(!blocks_touch(std::slice::from_ref(&a), t, 1., BlockPoint::default()));
    }
}

#[test]
fn pbc_refuses_to_silently_drop_block_areas() {
    use prpr::bin::BinaryWriter;
    let source = json!({"formatVersion":3,"offset":0,"judgeLineList":[]});
    let mut chart = parse_phigros(&source.to_string(), ChartExtra::default()).unwrap();
    let mut legacy = BinaryWriter::new(Vec::new());
    legacy.write(&chart).unwrap();
    assert!(!legacy.0.is_empty());
    chart.block_areas.push(area());
    let mut blocked = BinaryWriter::new(Vec::new());
    assert!(blocked.write(&chart).unwrap_err().to_string().contains("blockAreaList"));
    assert!(blocked.0.is_empty());
}

#[test]
fn rejects_unknown_easing_and_nonfinite_geometry() {
    let mut a = area();
    a.top_right_percentage.x = f32::NAN;
    assert!(a.validate().is_err());
    let mut a = area();
    a.rotate_events = serde_json::from_value(json!([
        {"time":0,"rotation":0,"anchor":{"x":0.5,"y":0.5},"easeType":15}
    ]))
    .unwrap();
    assert!(a.validate().is_err());
}
