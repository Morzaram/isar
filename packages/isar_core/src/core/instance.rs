use super::cursor::IsarCursor;
use super::de::IsarJsonImportVisitor;
use super::error::{IsarError, Result};
use super::insert::IsarInsert;
use super::query_builder::IsarQueryBuilder;
use super::reader::IsarReader;
use super::schema::IsarSchema;
use super::value::IsarValue;
use serde::Deserializer;

pub struct CompactCondition {
    pub min_file_size: u32,
    pub min_bytes: u32,
    pub min_ratio: f32,
}

pub trait IsarInstance: Sized {
    type Instance;

    type Txn;

    type Reader<'a>: IsarReader
    where
        Self: 'a;

    type Insert<'a>: IsarInsert<'a, Txn = Self::Txn>
    where
        Self: 'a;

    type QueryBuilder<'a>: IsarQueryBuilder<Query = Self::Query>
    where
        Self: 'a;

    type Query;

    type Cursor<'a>: IsarCursor<Reader<'a> = Self::Reader<'a>>
    where
        Self: 'a;

    fn get_instance(instance_id: u32) -> Option<Self::Instance>;

    fn get_name(&self) -> &str;

    fn get_dir(&self) -> &str;

    fn get_collections(&self) -> impl Iterator<Item = &str>;

    fn open_instance(
        instance_id: u32,
        name: &str,
        dir: &str,
        schema: IsarSchema,
        max_size_mib: u32,
        compact_condition: Option<CompactCondition>,
    ) -> Result<Self::Instance>;

    fn begin_txn(&self, write: bool) -> Result<Self::Txn>;

    fn commit_txn(&self, txn: Self::Txn) -> Result<()>;

    fn abort_txn(&self, txn: Self::Txn);

    fn get<'a>(
        &'a self,
        txn: &'a Self::Txn,
        collection_index: u16,
        id: i64,
    ) -> Result<Option<Self::Reader<'a>>>;

    fn insert(&self, txn: Self::Txn, collection_index: u16, count: u32)
        -> Result<Self::Insert<'_>>;

    fn update(
        &self,
        txn: &Self::Txn,
        collection_index: u16,
        id: i64,
        updates: &[(u16, Option<IsarValue>)],
    ) -> Result<bool>;

    fn delete(&self, txn: &Self::Txn, collection_index: u16, id: i64) -> Result<bool>;

    fn count(&self, txn: &Self::Txn, collection_index: u16) -> Result<u32>;

    fn clear(&self, txn: &Self::Txn, collection_index: u16) -> Result<()>;

    fn get_size(
        &self,
        txn: &Self::Txn,
        collection_index: u16,
        include_indexes: bool,
    ) -> Result<u64>;

    fn query(&self, collection_index: u16) -> Result<Self::QueryBuilder<'_>>;

    fn query_cursor<'a>(
        &'a self,
        txn: &'a Self::Txn,
        query: &'a Self::Query,
        offset: Option<u32>,
        limit: Option<u32>,
    ) -> Result<Self::Cursor<'_>>;

    fn query_aggregate(
        &self,
        txn: &Self::Txn,
        query: &Self::Query,
        aggregation: Aggregation,
        property_index: Option<u16>,
    ) -> Result<Option<IsarValue>>;

    fn query_update(
        &self,
        txn: &Self::Txn,
        query: &Self::Query,
        offset: Option<u32>,
        limit: Option<u32>,
        updates: &[(u16, Option<IsarValue>)],
    ) -> Result<u32>;

    fn query_delete(
        &self,
        txn: &Self::Txn,
        query: &Self::Query,
        offset: Option<u32>,
        limit: Option<u32>,
    ) -> Result<u32>;

    fn import_json<'a, T: Deserializer<'a>>(
        &self,
        txn: Self::Txn,
        collection_index: u16,
        deserializer: T,
    ) -> Result<(Self::Txn, u32)> {
        let (txn, count) = deserializer
            .deserialize_seq(IsarJsonImportVisitor::new(self, txn, collection_index))
            .map_err(|_| IsarError::JsonError {})?;
        Ok((txn, count))
    }

    fn copy(&self, path: &str) -> Result<()>;

    fn close(instance: Self::Instance, delete: bool) -> bool;
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Aggregation {
    Count,
    IsEmpty,
    Min,
    Max,
    Sum,
    Average,
}
