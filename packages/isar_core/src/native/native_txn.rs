use super::index::id_key::IdToBytes;
use super::index::index_key::IndexKey;
use super::mdbx::cursor::{Cursor, UnboundCursor};
use super::mdbx::cursor_iterator::CursorIterator;
use super::mdbx::db::Db;
use super::mdbx::env::Env;
use super::mdbx::txn::Txn;
use crate::core::error::{IsarError, Result};
use crate::core::watcher::ChangeSet;
use std::cell::{Cell, RefCell, RefMut};
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

pub struct NativeTxn {
    pub(crate) instance_id: u32,
    txn: Txn,
    active: Cell<bool>,
    buffer: Cell<Option<Vec<u8>>>,
    change_set: RefCell<ChangeSet>,
    unbound_cursors: RefCell<Vec<UnboundCursor>>,
}

impl NativeTxn {
    pub(crate) fn new(instance_id: u32, env: &Arc<Env>, write: bool) -> Result<Self> {
        let txn = env.txn(write)?;
        let txn = Self {
            instance_id,
            txn,
            active: Cell::new(true),
            buffer: Cell::new(None),
            change_set: RefCell::new(ChangeSet::new()),
            unbound_cursors: RefCell::new(Vec::new()),
        };
        Ok(txn)
    }

    pub(crate) fn get_cursor<'txn>(&'txn self, db: Db) -> Result<TxnCursor<'txn>> {
        if !self.active.get() {
            return Err(IsarError::TransactionClosed {});
        }

        let unbound = self
            .unbound_cursors
            .borrow_mut()
            .pop()
            .unwrap_or_else(UnboundCursor::new);
        let cursor = unbound.bind(&self.txn, db)?;

        Ok(TxnCursor {
            txn: self,
            cursor: Some(cursor),
        })
    }

    pub(crate) fn get_change_set(&self) -> RefMut<'_, ChangeSet> {
        self.change_set.borrow_mut()
    }

    #[inline]
    pub(crate) fn guard<T, F>(&self, job: F) -> Result<T>
    where
        F: FnOnce() -> Result<T>,
    {
        if !self.active.get() {
            return Err(IsarError::TransactionClosed {});
        }
        let result = job();
        if !result.is_ok() {
            self.active.replace(false);
        }
        result
    }

    pub(crate) fn open_db(&self, name: &str, int_key: bool, dup: bool) -> Result<Db> {
        if !self.active.get() {
            return Err(IsarError::TransactionClosed {});
        }
        Db::open(&self.txn, name, int_key, dup)
    }

    pub(crate) fn clear_db(&self, db: Db) -> Result<()> {
        if !self.active.get() {
            return Err(IsarError::TransactionClosed {});
        }
        db.clear(&self.txn)
    }

    pub(crate) fn drop_db(&self, db: Db) -> Result<()> {
        if !self.active.get() {
            return Err(IsarError::TransactionClosed {});
        }
        db.drop(&self.txn)
    }

    pub(crate) fn stat(&self, db: Db) -> Result<(u64, u64)> {
        if !self.active.get() {
            return Err(IsarError::TransactionClosed {});
        }
        db.stat(&self.txn)
    }

    pub(crate) fn commit(self) -> Result<()> {
        if !self.active.get() {
            return Err(IsarError::TransactionClosed {});
        }
        self.txn.commit()?;
        self.change_set.borrow_mut().notify_watchers();
        Ok(())
    }

    pub(crate) fn abort(self) {
        if self.active.get() {
            self.txn.abort()
        }
    }

    pub(crate) fn take_buffer(&self) -> Vec<u8> {
        self.buffer.replace(None).unwrap_or_else(Vec::new)
    }

    pub(crate) fn put_buffer(&self, mut buffer: Vec<u8>) {
        buffer.clear();
        self.buffer.replace(Some(buffer));
    }
}

pub(crate) struct TxnCursor<'txn> {
    txn: &'txn NativeTxn,
    cursor: Option<Cursor<'txn>>,
}

impl<'txn> TxnCursor<'txn> {
    pub fn iter_between_ids(
        self,
        start_id: i64,
        end_id: i64,
        duplicates: bool,
        skip_duplicates: bool,
    ) -> Result<CursorIterator<'txn, Self>> {
        CursorIterator::new(
            self,
            start_id.to_id_bytes().to_vec(),
            end_id.to_id_bytes().to_vec(),
            true,
            duplicates,
            skip_duplicates,
        )
    }

    pub fn iter_between(
        self,
        start_key: &IndexKey,
        end_key: &IndexKey,
        duplicates: bool,
        skip_duplicates: bool,
    ) -> Result<CursorIterator<'txn, Self>> {
        CursorIterator::new(
            self,
            start_key.to_bytes().to_vec(),
            end_key.to_bytes().to_vec(),
            false,
            duplicates,
            skip_duplicates,
        )
    }
}

impl<'txn> Deref for TxnCursor<'txn> {
    type Target = Cursor<'txn>;

    fn deref(&self) -> &Self::Target {
        self.cursor.as_ref().unwrap()
    }
}

impl<'txn> DerefMut for TxnCursor<'txn> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.cursor.as_mut().unwrap()
    }
}

impl<'txn> AsMut<Cursor<'txn>> for TxnCursor<'txn> {
    fn as_mut(&mut self) -> &mut Cursor<'txn> {
        self.cursor.as_mut().unwrap()
    }
}

impl<'txn> Drop for TxnCursor<'txn> {
    fn drop(&mut self) {
        if let Some(cursor) = self.cursor.take() {
            if self.txn.unbound_cursors.borrow().len() < 3 {
                self.txn.unbound_cursors.borrow_mut().push(cursor.unbind());
            }
        }
    }
}
