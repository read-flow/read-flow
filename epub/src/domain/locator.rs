use serde::Deserialize;
use serde::Serialize;

/// A renderer-independent reading position within a document.
///
/// Encodes: spine index -> DOM node path -> character offset.
/// Serializes to/from the `rf://` URI scheme.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Locator {
    pub spine_index: u32,
    pub node_path: Vec<u32>,
    pub char_offset: u32,
}

impl Locator {
    /// Serialize to an EPUB CFI string.
    ///
    /// Format: `epubcfi(/6/{step}[id]!/{path}:offset)`
    /// where step = (spine_index + 1) * 2 and each path step = (child_index + 1) * 2.
    /// The `[id]` assertion is included only when `spine_id` is `Some`.
    pub fn to_cfi(&self, spine_id: Option<&str>) -> String {
        let spine_step = (self.spine_index + 1) * 2;
        let assertion = spine_id.map(|id| format!("[{id}]")).unwrap_or_default();
        let path: String = self
            .node_path
            .iter()
            .map(|&idx| format!("/{}", (idx + 1) * 2))
            .collect();
        format!(
            "epubcfi(/6/{spine_step}{assertion}!{path}:{})",
            self.char_offset
        )
    }

    /// Parse a `Locator` from an EPUB CFI string.
    ///
    /// Odd-numbered path steps (text-node steps in CFI) are truncated — the
    /// path ends at the last even step, giving the containing element.
    /// Returns `None` if the string is not a recognisable EPUB CFI.
    pub fn from_cfi(cfi: &str) -> Option<Self> {
        let inner = cfi.strip_prefix("epubcfi(")?.strip_suffix(')')?;
        let (spine_part, doc_part) = inner.split_once('!')?;

        // Parse spine: "/6/{step}[optional-id]"
        let after_slash6 = spine_part.strip_prefix("/6/")?;
        let step_end = after_slash6
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(after_slash6.len());
        let spine_step: u32 = after_slash6[..step_end].parse().ok()?;
        if spine_step < 2 || !spine_step.is_multiple_of(2) {
            return None;
        }
        let spine_index = spine_step / 2 - 1;

        // Parse document path: "/{step}/...:{char_offset}"
        // The terminal ":{offset}" may be absent (some epub.js variants).
        let (path_str, char_offset) = match doc_part.rsplit_once(':') {
            Some((p, o)) => (p, o.parse::<u32>().unwrap_or(0)),
            None => (doc_part, 0),
        };

        // Map even CFI steps to 0-based child indices; stop at odd (text-node) steps.
        let node_path: Vec<u32> = path_str
            .split('/')
            .filter(|s| !s.is_empty())
            .map_while(|s| {
                let step_end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
                let step: u32 = s[..step_end].parse().ok()?;
                if !step.is_multiple_of(2) {
                    return None; // text-node step — stop here
                }
                if step < 2 {
                    return None;
                }
                Some(step / 2 - 1)
            })
            .collect();

        Some(Locator {
            spine_index,
            node_path,
            char_offset,
        })
    }

    /// Serialize to an `rf://` URI.
    ///
    /// Format: `rf://doc/<document_hash>/spine/<i>/node/<path>/char/<o>`
    pub fn to_uri(&self, document_hash: &str) -> String {
        let node = self
            .node_path
            .iter()
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join("/");
        format!(
            "rf://doc/{}/spine/{}/node/{}/char/{}",
            document_hash, self.spine_index, node, self.char_offset
        )
    }

    /// Parse a `Locator` from an `rf://` URI.
    ///
    /// Returns `None` if the URI is malformed.
    pub fn from_uri(uri: &str) -> Option<Self> {
        let rest = uri.strip_prefix("rf://doc/")?;

        let spine_marker = "/spine/";
        let spine_pos = rest.find(spine_marker)?;
        let after_spine = &rest[spine_pos + spine_marker.len()..];

        let node_marker = "/node/";
        let node_pos = after_spine.find(node_marker)?;
        let spine_index: u32 = after_spine[..node_pos].parse().ok()?;
        let after_node = &after_spine[node_pos + node_marker.len()..];

        let char_marker = "/char/";
        let char_pos = after_node.find(char_marker)?;
        let node_str = &after_node[..char_pos];
        let char_offset: u32 = after_node[char_pos + char_marker.len()..].parse().ok()?;

        let node_path = node_str
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|s| s.parse().ok())
            .collect::<Option<Vec<u32>>>()?;

        Some(Locator {
            spine_index,
            node_path,
            char_offset,
        })
    }
}

#[cfg(test)]
mod tests {
    use assert4rs::Assert;
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case(
        Locator { spine_index: 4, node_path: vec![0, 2, 5, 1], char_offset: 87 },
        "sha256:9a3e",
        "rf://doc/sha256:9a3e/spine/4/node/0/2/5/1/char/87"
    )]
    #[case(
        Locator { spine_index: 0, node_path: vec![], char_offset: 0 },
        "abc123",
        "rf://doc/abc123/spine/0/node//char/0"
    )]
    fn test_to_uri(#[case] locator: Locator, #[case] hash: &str, #[case] expected: &str) {
        Assert::that(locator.to_uri(hash)).is(expected);
    }

    #[rstest]
    #[case(
        "rf://doc/sha256:9a3e/spine/4/node/0/2/5/1/char/87",
        Some(Locator { spine_index: 4, node_path: vec![0, 2, 5, 1], char_offset: 87 })
    )]
    #[case(
        "rf://doc/abc123/spine/0/node//char/0",
        Some(Locator { spine_index: 0, node_path: vec![], char_offset: 0 })
    )]
    #[case("not-a-valid-uri", None)]
    #[case("rf://doc/hash/spine/notanum/node/0/char/0", None)]
    fn test_from_uri(#[case] uri: &str, #[case] expected: Option<Locator>) {
        Assert::that(Locator::from_uri(uri)).is(expected);
    }

    #[test]
    fn test_roundtrip() {
        let locator = Locator {
            spine_index: 7,
            node_path: vec![1, 3, 0],
            char_offset: 42,
        };
        let hash = "sha256:deadbeef";
        let uri = locator.to_uri(hash);
        let parsed = Locator::from_uri(&uri).expect("should parse");
        Assert::that(parsed).is(locator);
    }

    #[rstest]
    #[case(
        Locator { spine_index: 1, node_path: vec![1, 2], char_offset: 0 },
        None,
        "epubcfi(/6/4!/4/6:0)"
    )]
    #[case(
        Locator { spine_index: 0, node_path: vec![1, 0], char_offset: 0 },
        Some("chap01"),
        "epubcfi(/6/2[chap01]!/4/2:0)"
    )]
    #[case(
        Locator { spine_index: 3, node_path: vec![], char_offset: 0 },
        None,
        "epubcfi(/6/8!:0)"
    )]
    fn test_to_cfi(
        #[case] locator: Locator,
        #[case] spine_id: Option<&str>,
        #[case] expected: &str,
    ) {
        Assert::that(locator.to_cfi(spine_id)).is(expected);
    }

    #[rstest]
    #[case("epubcfi(/6/4!/4/6:0)",      Some(Locator { spine_index: 1, node_path: vec![1, 2], char_offset: 0 }))]
    #[case("epubcfi(/6/2[ch]!/4/2:0)",  Some(Locator { spine_index: 0, node_path: vec![1, 0], char_offset: 0 }))]
    // text-node step (/1) truncates path
    #[case("epubcfi(/6/4!/4/6/1:15)",   Some(Locator { spine_index: 1, node_path: vec![1, 2], char_offset: 15 }))]
    #[case("not-a-cfi", None)]
    #[case("epubcfi(/6/3!/4/6:0)", None)] // odd spine step
    fn test_from_cfi(#[case] cfi: &str, #[case] expected: Option<Locator>) {
        Assert::that(Locator::from_cfi(cfi)).is(expected);
    }

    #[test]
    fn test_cfi_roundtrip() {
        let locator = Locator {
            spine_index: 2,
            node_path: vec![1, 4, 1],
            char_offset: 0,
        };
        let cfi = locator.to_cfi(None);
        let parsed = Locator::from_cfi(&cfi).expect("should parse");
        Assert::that(parsed).is(locator);
    }
}
