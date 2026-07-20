use quick_xml::Reader;
use quick_xml::events::Event;

use crate::error::EpubError;
use crate::error::Result;

pub struct Container {
    pub rootfile_path: String,
}

impl Container {
    pub fn from_xml(xml: &[u8]) -> Result<Self> {
        let mut reader = Reader::from_reader(xml);
        let mut buf = Vec::new();

        loop {
            match reader.read_event_into(&mut buf)? {
                Event::Empty(ref e) | Event::Start(ref e) if e.name().as_ref() == b"rootfile" => {
                    for attr in e.attributes() {
                        let attr = attr.map_err(|e| {
                            EpubError::InvalidContainer(format!("bad attribute: {e}"))
                        })?;
                        if attr.key.as_ref() == b"full-path" {
                            let path = String::from_utf8_lossy(&attr.value).into_owned();
                            return Ok(Container {
                                rootfile_path: path,
                            });
                        }
                    }
                    return Err(EpubError::InvalidContainer(
                        "rootfile element missing full-path attribute".into(),
                    ));
                }
                Event::Eof => break,
                _ => {}
            }
            buf.clear();
        }

        Err(EpubError::InvalidContainer(
            "no rootfile element found".into(),
        ))
    }

    pub fn from_archive<R: std::io::Read + std::io::Seek>(
        archive: &mut zip::ZipArchive<R>,
    ) -> Result<Self> {
        let mut file = archive
            .by_name("META-INF/container.xml")
            .map_err(|_| EpubError::MissingFile("META-INF/container.xml".into()))?;
        let mut contents = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut contents)?;
        Self::from_xml(&contents)
    }
}

#[cfg(test)]
mod tests {
    use assert4rs::Assert;

    use super::*;

    const VALID_CONTAINER: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;

    #[test]
    fn parses_rootfile_path() {
        let container = Container::from_xml(VALID_CONTAINER).unwrap();
        Assert::that(container.rootfile_path).is("OEBPS/content.opf");
    }

    #[test]
    fn errors_on_missing_rootfile() {
        let xml = br#"<?xml version="1.0"?><container></container>"#;
        assert!(Container::from_xml(xml).is_err());
    }

    #[test]
    fn errors_on_missing_full_path() {
        let xml = br#"<?xml version="1.0"?>
<container><rootfiles><rootfile media-type="application/oebps-package+xml"/></rootfiles></container>"#;
        assert!(Container::from_xml(xml).is_err());
    }
}
