use std::{
    collections::{hash_map::Entry, HashMap, HashSet}, str::FromStr
};

use archive_organizer::api::{File, FileDataSource, ReadingStatus};

use crate::client::{Client, ClientSelector, FilesClientError};

pub struct Aggregator {
    clients: HashMap<ClientSelector, Client>,
}

impl Aggregator {
    pub fn new(clients: Vec<Client>) -> Self {
        Self {
            clients: clients
                .into_iter()
                .map(|client| (client.selector(), client))
                .collect(),
        }
    }

    pub fn add(&mut self, client: Client) -> Option<Client> {
        self.clients.insert(client.selector(), client)
    }

    pub async  fn aggregate(&self) -> Result<Documents, FilesClientError> {
	let mut documents = Documents::default();
	for (selector, client) in &self.clients {
	    for file in client.get_files().await? {
		documents.push((selector.clone(), file).into());
	    }
	}
	Ok(documents)
    }
}

pub enum DocumentType {
    Pdf,
    Epub,
    Mobi,
}

#[derive(Debug, thiserror::Error)]
#[error("unsupported document type: {0}")]
pub struct UnsupportedDocumentType(String);

impl FromStr for DocumentType {
    type Err = UnsupportedDocumentType;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lowercase = s.to_ascii_lowercase();
        match lowercase.as_str() {
            "pdf" => Ok(Self::Pdf),
            "epub" => Ok(Self::Epub),
            "mobi" => Ok(Self::Mobi),
            _ => Err(UnsupportedDocumentType(lowercase)),
        }
    }
}

pub struct DocumentMetadata {
    pub type_: DocumentType,
    pub size: i32,
    pub fingerprint: String,
    pub tags: Vec<String>,
    pub status: ReadingStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DocumentSource {
    pub id: i32,
    pub path: String,
    pub client: ClientSelector,
}

pub struct Document {
    pub metadata: DocumentMetadata,
    pub sources: HashSet<DocumentSource>,
}

#[derive(Default)]
pub struct Documents(HashMap<String, Document>);

impl Documents {
    pub fn push(&mut self, document: Document) {
        match self.0.entry(document.metadata.fingerprint.clone()) {
            Entry::Occupied(mut occupied_entry) => {
                occupied_entry.get_mut().sources.extend(document.sources)
            }
            Entry::Vacant(vacant_entry) => {
                vacant_entry.insert(document);
            }
        }
    }
}

impl From<(ClientSelector, File)> for Document {
    fn from((client, file): (ClientSelector, File)) -> Self {
        Document {
            metadata: DocumentMetadata {
                type_: file.type_.parse().unwrap(), // safe because only supported types are stored in the database
                size: file.size,
                fingerprint: file.fingerprint,
                tags: file.tags,
                status: file.status,
            },
            sources: HashSet::from_iter([DocumentSource {
                id: file.id,
                path: file.path,
                client,
            }]),
        }
    }
}
