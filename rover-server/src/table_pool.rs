use mlua::{Lua, Table};
use std::cell::RefCell;

#[derive(Debug)]
pub struct LuaTablePool {
    available: RefCell<Vec<Table>>,
    capacity: usize,
    total_created: RefCell<usize>,
    total_reused: RefCell<usize>,
}

impl LuaTablePool {
    pub fn new(capacity: usize) -> Self {
        Self {
            available: RefCell::new(Vec::with_capacity(capacity)),
            capacity,
            total_created: RefCell::new(0),
            total_reused: RefCell::new(0),
        }
    }

    pub fn get(&self, lua: &Lua, narr: usize, nrec: usize) -> Table {
        let mut available = self.available.borrow_mut();
        
        if let Some(table) = available.pop() {
            *self.total_reused.borrow_mut() += 1;
            
            table.clear();
            
            table
        } else {
            *self.total_created.borrow_mut() += 1;
            
            lua.create_table_with_capacity(narr, nrec)
                .expect("Failed to create Lua table")
        }
    }

    pub fn put(&self, table: Table) {
        let mut available = self.available.borrow_mut();
        
        if available.len() < self.capacity {
            available.push(table);
        }
    }

    pub fn stats(&self) -> PoolStats {
        let available = self.available.borrow();
        PoolStats {
            available: available.len(),
            capacity: self.capacity,
            total_created: *self.total_created.borrow(),
            total_reused: *self.total_reused.borrow(),
            reuse_rate: if *self.total_created.borrow() > 0 {
                *self.total_reused.borrow() as f64 / *self.total_created.borrow() as f64
            } else {
                0.0
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct PoolStats {
    pub available: usize,
    pub capacity: usize,
    pub total_created: usize,
    pub total_reused: usize,
    pub reuse_rate: f64,
}
