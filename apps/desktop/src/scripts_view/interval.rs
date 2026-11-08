//! Human-readable intervals; persisted values stay exact whole seconds.
pub(super) fn split(seconds: u64) -> (u64, &'static str) {
    let seconds = if seconds == 0 { 86_400 } else { seconds };
    for (unit, factor) in [("天", 86_400), ("小时", 3_600), ("分钟", 60)] {
        if seconds.is_multiple_of(factor) {
            return (seconds / factor, unit);
        }
    }
    (seconds, "秒")
}

pub(super) fn to_seconds(value: u64, unit: &str) -> Result<u64, String> {
    let factor = match unit {
        "秒" => 1,
        "分钟" => 60,
        "小时" => 3_600,
        "天" => 86_400,
        _ => return Err("请选择时间单位".into()),
    };
    value
        .checked_mul(factor)
        .filter(|value| *value > 0)
        .ok_or_else(|| "更新间隔超出有效范围".into())
}

pub(super) fn display(seconds: u64) -> String {
    let (value, unit) = split(seconds);
    format!("{value} {unit}")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn existing_intervals_round_trip_without_rounding() {
        for seconds in [1, 59, 60, 90, 3_600, 5_400, 86_400, u64::MAX] {
            let (value, unit) = split(seconds);
            assert_eq!(to_seconds(value, unit).unwrap(), seconds);
        }
        assert_eq!(display(86_400), "1 天");
        assert_eq!(display(7_200), "2 小时");
        assert!(to_seconds(u64::MAX, "天").is_err());
        assert!(to_seconds(0, "秒").is_err());
    }
}
