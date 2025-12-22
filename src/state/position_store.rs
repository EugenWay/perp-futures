// src/state/position_store.rs

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use crate::types::{AccountId, AssetId, MarketId, Side, Timestamp, TokenAmount, Usd};

/// Ключ позиции: уникально определяет позицию пользователя.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PositionKey {
    pub account: AccountId,
    pub market_id: MarketId,
    pub collateral_token: AssetId,
    pub side: Side,
}

#[derive(Clone, Debug)]
pub struct Position {
    pub key: PositionKey,

    pub size_usd: Usd,

    pub size_tokens: TokenAmount,

    pub collateral_amount: TokenAmount,

    pub pending_impact_tokens: TokenAmount,

    pub funding_index: i128,

    pub borrowing_index: i128,

    pub opened_at: Timestamp,

    pub last_updated_at: Timestamp,
}

/// Хранилище позиций.
#[derive(Default)]
pub struct PositionStore {
    positions: HashMap<PositionKey, Position>,
}

impl PositionStore {
    pub fn new() -> Self {
        Self {
            positions: HashMap::new(),
        }
    }

    pub fn get(&self, key: &PositionKey) -> Option<&Position> {
        self.positions.get(key)
    }

    pub fn get_mut(&mut self, key: &PositionKey) -> Option<&mut Position> {
        self.positions.get_mut(key)
    }

    pub fn upsert(&mut self, position: Position) {
        self.positions.insert(position.key, position);
    }

    pub fn remove(&mut self, key: &PositionKey) -> Option<Position> {
        self.positions.remove(key)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&PositionKey, &Position)> {
        self.positions.iter()
    }

    pub fn get_or_insert_with<F>(&mut self, key: PositionKey, f: F) -> &mut Position
    where
        F: FnOnce(PositionKey) -> Position,
    {
        match self.positions.entry(key) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => {
                let k = *e.key(); // PositionKey: Copy
                e.insert(f(k))
            }
        }
    }
}
