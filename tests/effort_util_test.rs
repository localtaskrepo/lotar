use lotar::utils::effort::{EffortKind, parse_effort};

#[test]
fn parse_time_minutes_and_combined() {
    let e = parse_effort("90m").unwrap();
    match e.kind {
        EffortKind::TimeHours(h) => assert!((h - 1.5).abs() < 1e-9),
        _ => panic!("expected time"),
    }

    let e2 = parse_effort("1d 2h").unwrap();
    match e2.kind {
        EffortKind::TimeHours(h) => assert!((h - 10.0).abs() < 1e-9),
        _ => panic!("expected time"),
    }

    let e3 = parse_effort("1 hr 30 min").unwrap();
    match e3.kind {
        EffortKind::TimeHours(h) => assert!((h - 1.5).abs() < 1e-9),
        _ => panic!("expected time"),
    }

    let e4 = parse_effort("2 weeks").unwrap();
    match e4.kind {
        EffortKind::TimeHours(h) => assert!((h - 80.0).abs() < 1e-9),
        _ => panic!("expected time"),
    }
}

#[test]
fn parse_points_and_mixed_invalid() {
    let p = parse_effort("5pt").unwrap();
    match p.kind {
        EffortKind::Points(n) => assert_eq!(n, 5.0),
        _ => panic!("expected points"),
    }

    let p2 = parse_effort("8").unwrap();
    match p2.kind {
        EffortKind::Points(n) => assert_eq!(n, 8.0),
        _ => panic!("expected points"),
    }

    let p3 = parse_effort("8 points").unwrap();
    match p3.kind {
        EffortKind::Points(n) => assert_eq!(n, 8.0),
        _ => panic!("expected points"),
    }

    assert!(parse_effort("1h 3pt").is_err());
}
