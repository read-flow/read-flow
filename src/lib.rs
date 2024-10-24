pub mod api;
pub mod client;
pub mod db;
#[cfg(feature = "gui")]
pub mod gui;
pub mod scan;
pub mod serve;

use std::{hash::Hash, path::PathBuf};

use indexmap::IndexMap;

fn to_unique_file(file_path: &mut PathBuf, extension: &str) {
    let mut index: usize = 1;

    while file_path.exists() {
        if index > 1 {
            file_path.set_extension("");
        }
        file_path.set_extension(format!("{index}.{extension}"));
        index += 1;
    }
}

fn extension_of(filename: &str) -> Option<&str> {
    filename.split(".").last()
}

pub fn to_buckets<K, V, F>(iterator: impl Iterator<Item = V>, to_key: F) -> IndexMap<K, Vec<V>>
where
    K: Hash + Eq,
    F: Fn(&V) -> K,
{
    let mut buckets = IndexMap::default();

    for value in iterator {
        let key = to_key(&value);
        let entry = buckets.entry(key).or_insert(vec![]);
        entry.push(value)
    }

    buckets
}

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;

    use super::*;

    #[test]
    fn test_buckets() {
        let input = vec![0, 1, 2, 2, 3, 4, 5, 5, 6, 5, 6, 7, 8];
        let actual = to_buckets(input.into_iter(), |x| *x);
        let expected: IndexMap<i32, Vec<i32>> = vec![
            (0, vec![0]),
            (1, vec![1]),
            (2, vec![2, 2]),
            (3, vec![3]),
            (4, vec![4]),
            (5, vec![5, 5, 5]),
            (6, vec![6, 6]),
            (7, vec![7]),
            (8, vec![8]),
        ]
        .into_iter()
        .collect();
        assert_eq!(actual, expected);

        let actual: IndexMap<i32, Vec<i32>> = actual
            .into_iter()
            .filter(|(_, value)| value.len() > 1)
            .collect();
        let expected: IndexMap<i32, Vec<i32>> =
            vec![(2, vec![2, 2]), (5, vec![5, 5, 5]), (6, vec![6, 6])]
                .into_iter()
                .collect();
        assert_eq!(actual, expected);
    }
}
