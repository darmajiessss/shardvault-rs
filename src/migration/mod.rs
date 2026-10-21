// E:\shardvault-rs\src\migration\mod.rs
// Modul Migrasi & Struktur Database ShardVault-RS

pub mod ddl;
pub mod ledger;
pub mod runner;

pub use ddl::{
    detect_schema_drift, introspect_schema, ColumnInfo, ForeignKeyInfo, IndexInfo, SchemaManifest, TableSchema,
};
pub use ledger::{get_applied_migrations, init_migration_ledger, record_migration, MigrationRecord};
pub use runner::{Migration, MigrationReport, MigrationRunner, MigrationTarget};
