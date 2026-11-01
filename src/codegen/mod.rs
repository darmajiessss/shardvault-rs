// E:\shardvault-rs\src\codegen\mod.rs
// Modul Visual Schema Builder, DrawDB ER AST, Smart Diff Planner, & Multi-Target Code Generator

pub mod ast;
pub mod ddl_gen;
pub mod diff_gen;
pub mod rust_gen;
pub mod ts_gen;

pub use ast::{
    Cardinality, ColumnBlueprint, ForeignKeyAction, ForeignKeyBlueprint, FriendlyFieldType,
    IndexBlueprint, IssueSeverity, ModularTablePreset, NoteBlueprint, SchemaBlueprint, SchemaIssue,
    SqliteAffinity, SubjectAreaBlueprint, TableBlueprint, TableScope,
};
pub use ddl_gen::{compile_blueprint_ddl, compile_table_ddl, quote_ident, SqlDialect};
pub use diff_gen::{diff_blueprint_against_manifest, DiffActionItem, DiffActionKind, SchemaDiffPlan};
pub use rust_gen::generate_rust_code;
pub use ts_gen::{export_blueprint_json, generate_typescript_code};
