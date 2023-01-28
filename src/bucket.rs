use chrono::{DateTime, Utc};
use serde::Serialize;
use turbosql::{execute, select, Turbosql};

#[derive(Debug, Serialize, Turbosql, Default, Clone)]
pub struct Bucket {
    pub rowid: Option<i64>,
    pub name: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Default)]
pub struct BucketWithStats {
    pub rowid: Option<i64>,
    pub name: Option<String>,
    pub lot_count: Option<i64>,
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

    pub fn list() -> Result<Vec<BucketWithStats>, turbosql::Error> {
        let bs = select!(Vec<BucketWithStats> "SELECT b.rowid, b.name, count(lot.rowid) AS lot_count FROM bucket b LEFT JOIN lot on lot.bucket_id = b.rowid GROUP BY b.rowid ORDER BY updated_at DESC");
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

    pub fn delete(name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let bucket = Self::get_by_name(name);
        match bucket {
            Ok(bucket) => {
                let lots =
                    select!(i64 "SELECT COUNT(*) FROM lot WHERE bucket_id = ?", bucket.rowid);
                if lots.unwrap() > 0 {
                    return Err("Bucket is not empty".into());
                } else {
                    execute!("DELETE FROM bucket WHERE rowid = ?", bucket.rowid)?;
                }

                Ok(())
            }
            Err(_) => Err("Bucket does not exist".into()),
        }
    }
}
