pub fn find_with_previous<T>(
    iter: impl Iterator<Item = T>,
    is_match: impl Fn(&T) -> bool,
) -> Option<(T, T)> {
    let (prev, current) = iter.fold((None, None), |acc, current| match acc {
        (Some(prev), None) if is_match(&current) => (Some(prev), Some(current)),
        (_, None) => (Some(current), None),
        result => result,
    });
    if let (Some(prev), Some(current)) = (prev, current) {
        Some((prev, current))
    } else {
        None
    }
}

pub fn find_with_next<T>(
    iter: impl Iterator<Item = T>,
    is_match: impl Fn(&T) -> bool,
) -> Option<(T, T)> {
    let (current, next) = iter.fold((None, None), |acc, current| match acc {
        (None, None) if is_match(&current) => (Some(current), None),
        (Some(prev), None) => (Some(prev), Some(current)),
        result => result,
    });
    if let (Some(current), Some(next)) = (current, next) {
        Some((current, next))
    } else {
        None
    }
}
