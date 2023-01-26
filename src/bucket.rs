use chrono::{DateTime, Utc};
use serde::Serialize;
use turbosql::{select, Turbosql};

#[derive(Debug, Serialize, Turbosql, Default, Clone)]
pub struct Bucket {
    pub rowid: Option<i64>,
    pub name: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl Bucket {
    pub fn new(name: &str) -> Self {
        Self {
            name: Some(name.to_string()),
            created_at: Some(Utc::now()),
            ..Default::default()
        }
    }

    pub fn get_by_id(id: &i64) -> Result<Self, turbosql::Error> {
        select!(Bucket "WHERE rowid = ? LIMIT 1", id)
    }

    pub fn get_by_name(name: &str) -> Result<Self, turbosql::Error> {
        select!(Bucket "WHERE name = ? LIMIT 1", name)
    }

    pub fn list() -> Result<Vec<Bucket>, turbosql::Error> {
        let bs = select!(Vec<Bucket> "ORDER BY updated_at DESC");
        match bs {
            Ok(b) => Ok(b),
            Err(e) => Err(e),
        }
    }

    pub fn create(&mut self) -> Result<i64, Box<dyn std::error::Error>> {
        let bucket = select!(Bucket "WHERE name = ?", self.name);
        if bucket.is_err() {
            let rowid = self.insert()?;
            Ok(rowid)
        } else {
            Err("Bucket already exists".into())
        }
    }
    pub fn update_name(old_name: &str, name: &str) -> Result<Bucket, Box<dyn std::error::Error>> {
        let bucket = select!(Bucket "WHERE name = ?", old_name);
        match bucket {
            Ok(bucket) => {
                let mut bucket = bucket;
                bucket.name = Some(name.to_string());
                bucket.updated_at = Some(Utc::now());
                bucket.update()?;
                Ok(bucket)
            }
            Err(_) => Err("Bucket does not exist".into()),
        }
    }
}
