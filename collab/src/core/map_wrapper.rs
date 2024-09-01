use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde::Serialize;
use yrs::block::Prelim;
use yrs::types::ToJson;
use yrs::{
    ArrayPrelim, ArrayRef, Map, MapPrelim, MapRef,
    ReadTxn, TextPrelim, Transaction, TransactionMut,
};
use crate::core::array_wrapper::ArrayRefWrapper;
use crate::core::text_wrapper::TextRefWrapper;
use crate::core::value::YrsValueExtension;
use crate::preclude::*;
use crate::util::any_to_json_value;

pub trait CustomMapRef {
    fn from_map_ref(map_ref: MapRefWrapper) -> Self;
}

impl CustomMapRef for MapRefWrapper {
    fn from_map_ref(map_ref: MapRefWrapper) -> Self {
        map_ref
    }
}

#[derive(Clone)]
pub struct MapRefWrapper {
    map_ref: MapRef,
    pub collab_ctx: CollabContext,
}

impl MapRefWrapper {
    pub fn new(map_ref: MapRef, collab_ctx: CollabContext) -> Self {
        Self {
            collab_ctx,
            map_ref,
        }
    }

    pub fn into_inner(self) -> MapRef {
        self.map_ref
    }

    pub fn insert<V: Prelim>(&self, key: &str, value: V) {
        self.collab_ctx.with_transact_mut(|txn| {
            self.map_ref.insert(txn, key, value);
        })
    }

    pub fn insert_with_txn<V: Prelim>(&self, txn: &mut TransactionMut, key: &str, value: V) {
        self.map_ref.insert(txn, key, value);
    }

    pub fn insert_text_with_txn(&self, txn: &mut TransactionMut, key: &str) -> TextRefWrapper {
        let text = TextPrelim::new("");
        let text_ref = self.map_ref.insert(txn, key, text);
        TextRefWrapper::new(text_ref, self.collab_ctx.clone())
    }

    pub fn insert_array<V: Into<Any>>(&self, key: &str, values: Vec<V>) -> ArrayRefWrapper {
        self.with_transact_mut(|txn| self.insert_array_with_txn(txn, key, values))
    }

    pub fn insert_map<T: Into<MapPrelim>>(&self, key: &str, value: T) {
        self.with_transact_mut(|txn| self.insert_map_with_txn(txn, key, value));
    }

    pub fn insert_map_with_txn<T: Into<MapPrelim>>(
        &self,
        txn: &mut TransactionMut,
        key: &str,
        value: T,
    ) {
        self.map_ref.insert(txn, key, value.into());
    }

    pub fn insert_array_with_txn<V: Into<Any>>(
        &self,
        txn: &mut TransactionMut,
        key: &str,
        values: Vec<V>,
    ) -> ArrayRefWrapper {
        let array = self.map_ref.insert(
            txn,
            key,
            ArrayPrelim::from_iter(values.into_iter().map(|v| In::Any(v.into()))),
        );
        ArrayRefWrapper::new(array, self.collab_ctx.clone())
    }

    pub fn create_array_if_not_exist_with_txn<V: Into<Any>, K: AsRef<str>>(
        &self,
        txn: &mut TransactionMut,
        key: K,
        values: Vec<V>,
    ) -> ArrayRefWrapper {
        let array_ref = self.map_ref.create_array_if_not_exist_with_txn(
            txn,
            key.as_ref(),
            values.into_iter().map(|v| In::Any(v.into())).collect(),
        );
        ArrayRefWrapper::new(array_ref, self.collab_ctx.clone())
    }

    pub fn create_map_with_txn_if_not_exist(
        &self,
        txn: &mut TransactionMut,
        key: &str,
    ) -> MapRefWrapper {
        let map_ref = self.map_ref.create_map_if_not_exist_with_txn(txn, key);
        MapRefWrapper::new(map_ref, self.collab_ctx.clone())
    }

    pub fn get_or_insert_array_with_txn<V: Into<Any>>(
        &self,
        txn: &mut TransactionMut,
        key: &str,
    ) -> ArrayRefWrapper {
        self
            .get_array_ref_with_txn(txn, key)
            .unwrap_or_else(|| self.insert_array_with_txn::<V>(txn, key, vec![]))
    }

    pub fn create_map_with_txn(&self, txn: &mut TransactionMut, key: &str) -> MapRefWrapper {
        let map = MapPrelim::default();
        let map_ref = self.map_ref.insert(txn, key, map);
        MapRefWrapper::new(map_ref, self.collab_ctx.clone())
    }

    pub fn get_map_with_txn<T: ReadTxn, K: AsRef<str>>(
        &self,
        txn: &T,
        key: K,
    ) -> Option<MapRefWrapper> {
        let a = self.map_ref.get(txn, key.as_ref());
        if let Some(YrsValue::YMap(map_ref)) = a {
            return Some(MapRefWrapper::new(map_ref, self.collab_ctx.clone()));
        }
        None
    }

    pub fn get_array_ref_with_txn<T: ReadTxn, K: AsRef<str>>(
        &self,
        txn: &T,
        key: K,
    ) -> Option<ArrayRefWrapper> {
        let value = self.map_ref.get(txn, key.as_ref());
        let array_ref = value?.to_yarray().cloned()?;
        Some(ArrayRefWrapper::new(array_ref, self.collab_ctx.clone()))
    }

    pub fn get_text_ref_with_txn<T: ReadTxn>(&self, txn: &T, key: &str) -> Option<TextRefWrapper> {
        let text_ref = self
            .map_ref
            .get(txn, key)
            .map(|value| value.to_ytext().cloned())??;
        Some(TextRefWrapper::new(text_ref, self.collab_ctx.clone()))
    }

    pub fn insert_json<T: Serialize>(&self, key: &str, value: T) {
        let value = serde_json::to_value(&value).unwrap();
        self.collab_ctx.with_transact_mut(|txn| {
            insert_json_value_to_map_ref(key, &value, self.map_ref.clone(), txn);
        });
    }

    pub fn insert_json_with_txn<T: Serialize>(&self, txn: &mut TransactionMut, key: &str, value: T) {
        let value = serde_json::to_value(&value).unwrap();
        if let Some(map_ref) = self.get_map_with_txn(txn, key) {
            insert_json_value_to_map_ref(key, &value, map_ref.into_inner(), txn);
        }
    }

    pub fn get_json<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.get_json_with_txn(&self.collab_ctx.transact(), key)
    }

    pub fn get_json_with_txn<T: DeserializeOwned>(&self, txn: &Transaction, key: &str) -> Option<T> {
        let map_ref = self.get_map_with_txn(txn, key)?;
        let json_value = any_to_json_value(map_ref.into_inner().to_json(txn)).ok()?;
        serde_json::from_value::<T>(json_value).ok()
    }

    pub fn transact(&self) -> Transaction {
        self.collab_ctx.transact()
    }

    pub fn with_transact_mut<F, T>(&self, f: F) -> T
    where
        F: FnOnce(&mut TransactionMut) -> T,
    {
        self.collab_ctx.with_transact_mut(f)
    }

    pub fn to_json_str(&self) -> String {
        let txn = self.collab_ctx.transact();
        let value = self.map_ref.to_json(&txn);
        let mut json_str = String::new();
        value.to_json(&mut json_str);
        json_str
    }
    pub fn to_json_value(&self) -> anyhow::Result<serde_json::Value> {
        let txn = self.collab_ctx.transact();
        let value = self.map_ref.to_json(&txn);
        any_to_json_value(value)
    }
}

impl MapRefExtension for MapRefWrapper {
    fn map_ref(&self) -> &MapRef {
        &self.map_ref
    }
}
