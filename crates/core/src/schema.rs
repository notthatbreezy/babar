//! Core schema symbols for the typed query system.
//!
//! The typed-query macros will resolve against generated or handwritten Rust
//! schema symbols rather than string-heavy metadata. These primitives are
//! intentionally small and const-friendly so schema fixtures can be authored as
//! ordinary `const` values.

use core::fmt;
use core::marker::PhantomData;

use crate::types::{self, Oid, Type};

/// SQL type metadata carried by schema symbols.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct SqlType(Type);

impl SqlType {
    /// `bool`.
    pub const BOOL: Self = Self::fixed(types::BOOL, "bool");
    /// `bytea`.
    pub const BYTEA: Self = Self::fixed(types::BYTEA, "bytea");
    /// `varchar`.
    pub const VARCHAR: Self = Self::fixed(types::VARCHAR, "varchar");
    /// `text`.
    pub const TEXT: Self = Self::fixed(types::TEXT, "text");
    /// `int2`.
    pub const INT2: Self = Self::fixed(types::INT2, "int2");
    /// `int4`.
    pub const INT4: Self = Self::fixed(types::INT4, "int4");
    /// `int8`.
    pub const INT8: Self = Self::fixed(types::INT8, "int8");
    /// `float4`.
    pub const FLOAT4: Self = Self::fixed(types::FLOAT4, "float4");
    /// `float8`.
    pub const FLOAT8: Self = Self::fixed(types::FLOAT8, "float8");
    /// `uuid`.
    pub const UUID: Self = Self::fixed(types::UUID, "uuid");
    /// `date`.
    pub const DATE: Self = Self::fixed(types::DATE, "date");
    /// `time`.
    pub const TIME: Self = Self::fixed(types::TIME, "time");
    /// `timestamp`.
    pub const TIMESTAMP: Self = Self::fixed(types::TIMESTAMP, "timestamp");
    /// `timestamptz`.
    pub const TIMESTAMPTZ: Self = Self::fixed(types::TIMESTAMPTZ, "timestamptz");
    /// `json`.
    pub const JSON: Self = Self::fixed(types::JSON, "json");
    /// `jsonb`.
    pub const JSONB: Self = Self::fixed(types::JSONB, "jsonb");
    /// `numeric`.
    pub const NUMERIC: Self = Self::fixed(types::NUMERIC, "numeric");
    /// Dynamic PostGIS `geometry`.
    pub const GEOMETRY: Self = Self::extension("geometry", "postgis");

    /// Build a marker for a built-in type with a stable OID.
    pub const fn fixed(oid: Oid, name: &'static str) -> Self {
        Self(Type::fixed(oid, name))
    }

    /// Build a marker for an extension-defined type.
    pub const fn extension(name: &'static str, extension: &'static str) -> Self {
        Self(Type::extension(name, extension))
    }

    /// Build a marker resolved by SQL type name alone.
    pub const fn unresolved(name: &'static str) -> Self {
        Self(Type::unresolved(name))
    }

    /// Underlying runtime type metadata.
    pub const fn metadata(self) -> Type {
        self.0
    }

    /// The concrete OID when already known.
    pub const fn oid(self) -> Oid {
        self.0.oid()
    }

    /// SQL type name.
    pub const fn name(self) -> &'static str {
        self.0.name()
    }

    /// Extension name for dynamic types.
    pub const fn extension_name(self) -> Option<&'static str> {
        self.0.extension_name()
    }

    /// Whether the type already has a concrete OID.
    pub const fn is_resolved(self) -> bool {
        self.0.is_resolved()
    }
}

impl From<Type> for SqlType {
    fn from(value: Type) -> Self {
        Self(value)
    }
}

impl From<SqlType> for Type {
    fn from(value: SqlType) -> Self {
        value.0
    }
}

/// Base or widened NULL semantics for schema-bound values.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Nullability {
    /// The value is known not to be `NULL`.
    NonNull,
    /// The value may be `NULL`.
    Nullable,
}

impl Nullability {
    /// Build a marker from a boolean flag.
    pub const fn from_nullable(is_nullable: bool) -> Self {
        if is_nullable {
            Self::Nullable
        } else {
            Self::NonNull
        }
    }

    /// Whether the value may be `NULL`.
    pub const fn is_nullable(self) -> bool {
        match self {
            Self::NonNull => false,
            Self::Nullable => true,
        }
    }

    /// Combine base column nullability with binding-level nullability.
    #[must_use]
    pub const fn widen(self, other: Self) -> Self {
        if self.is_nullable() || other.is_nullable() {
            Self::Nullable
        } else {
            Self::NonNull
        }
    }
}

/// Narrow semantic markers carried by authored schema declarations.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ColumnSemantics {
    /// No additional semantic marker.
    Ordinary,
    /// The column is the table's primary key.
    PrimaryKey,
}

impl ColumnSemantics {
    /// Whether this column is marked as a primary key.
    pub const fn is_primary_key(self) -> bool {
        matches!(self, Self::PrimaryKey)
    }
}

/// Schema declaration metadata for a single column.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ColumnDef {
    name: &'static str,
    sql_type: SqlType,
    nullability: Nullability,
    semantics: ColumnSemantics,
}

impl ColumnDef {
    /// Create a column definition without additional semantic markers.
    pub const fn new(name: &'static str, sql_type: SqlType, nullability: Nullability) -> Self {
        Self::with_semantics(name, sql_type, nullability, ColumnSemantics::Ordinary)
    }

    /// Create a column definition with an explicit semantic marker.
    pub const fn with_semantics(
        name: &'static str,
        sql_type: SqlType,
        nullability: Nullability,
        semantics: ColumnSemantics,
    ) -> Self {
        Self {
            name,
            sql_type,
            nullability,
            semantics,
        }
    }

    /// Column name.
    pub const fn name(self) -> &'static str {
        self.name
    }

    /// Declared SQL type.
    pub const fn sql_type(self) -> SqlType {
        self.sql_type
    }

    /// Column nullability.
    pub const fn nullability(self) -> Nullability {
        self.nullability
    }

    /// Field-level semantic marker.
    pub const fn semantics(self) -> ColumnSemantics {
        self.semantics
    }

    /// Materialize a table-bound column symbol from this definition.
    pub const fn materialize<T>(self, table: TableRef<T>) -> Column<T> {
        Column::with_semantics(
            table,
            self.name,
            self.sql_type,
            self.nullability,
            self.semantics,
        )
    }
}

/// Schema declaration metadata for a single table.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct TableDef {
    schema: Option<&'static str>,
    name: &'static str,
    columns: &'static [ColumnDef],
}

impl TableDef {
    /// Create a table definition from authored column metadata.
    pub const fn new(
        schema: Option<&'static str>,
        name: &'static str,
        columns: &'static [ColumnDef],
    ) -> Self {
        Self {
            schema,
            name,
            columns,
        }
    }

    /// Optional schema name such as `public`.
    pub const fn schema_name(self) -> Option<&'static str> {
        self.schema
    }

    /// Unqualified table name such as `users`.
    pub const fn name(self) -> &'static str {
        self.name
    }

    /// Authored column definitions for this table.
    pub const fn columns(self) -> &'static [ColumnDef] {
        self.columns
    }

    /// Materialize a typed table symbol from this definition.
    pub const fn table_ref<T>(self) -> TableRef<T> {
        TableRef::new(self.schema, self.name)
    }
}

/// Schema declaration metadata for an authored schema module.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct SchemaDef {
    tables: &'static [TableDef],
}

impl SchemaDef {
    /// Create a schema definition from authored table metadata.
    pub const fn new(tables: &'static [TableDef]) -> Self {
        Self { tables }
    }

    /// Authored table definitions.
    pub const fn tables(self) -> &'static [TableDef] {
        self.tables
    }

    /// Look up a table by optional schema and table name.
    pub fn find_table(self, schema: Option<&str>, table_name: &str) -> Option<TableDef> {
        self.tables
            .iter()
            .copied()
            .find(|table| table.schema_name() == schema && table.name() == table_name)
    }
}

/// A schema table symbol.
pub struct TableRef<T> {
    schema: Option<&'static str>,
    name: &'static str,
    marker: PhantomData<fn() -> T>,
}

impl<T> Copy for TableRef<T> {}

impl<T> Clone for TableRef<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> TableRef<T> {
    /// Create a table symbol.
    pub const fn new(schema: Option<&'static str>, name: &'static str) -> Self {
        Self {
            schema,
            name,
            marker: PhantomData,
        }
    }

    /// Optional schema name such as `public`.
    pub const fn schema_name(self) -> Option<&'static str> {
        self.schema
    }

    /// Unqualified table name such as `users`.
    pub const fn name(self) -> &'static str {
        self.name
    }

    /// Bind the table under its own table name.
    pub const fn bind(self) -> Binding<T> {
        Binding::new(self, self.name, Nullability::NonNull)
    }

    /// Bind the table under an explicit alias.
    pub const fn alias(self, binding_name: &'static str) -> Binding<T> {
        Binding::new(self, binding_name, Nullability::NonNull)
    }
}

impl<T> fmt::Debug for TableRef<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TableRef")
            .field("schema", &self.schema)
            .field("name", &self.name)
            .finish()
    }
}

impl<T> PartialEq for TableRef<T> {
    fn eq(&self, other: &Self) -> bool {
        self.schema == other.schema && self.name == other.name
    }
}

impl<T> Eq for TableRef<T> {}

impl<T> core::hash::Hash for TableRef<T> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.schema.hash(state);
        self.name.hash(state);
    }
}

impl<T> fmt::Display for TableRef<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.schema {
            Some(schema) => write!(f, "{schema}.{}", self.name),
            None => f.write_str(self.name),
        }
    }
}

/// A schema column symbol.
pub struct Column<T> {
    table: TableRef<T>,
    name: &'static str,
    sql_type: SqlType,
    nullability: Nullability,
    semantics: ColumnSemantics,
}

impl<T> Copy for Column<T> {}

impl<T> Clone for Column<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Column<T> {
    /// Create a column symbol.
    pub const fn new(
        table: TableRef<T>,
        name: &'static str,
        sql_type: SqlType,
        nullability: Nullability,
    ) -> Self {
        Self::with_semantics(
            table,
            name,
            sql_type,
            nullability,
            ColumnSemantics::Ordinary,
        )
    }

    /// Create a column symbol with an explicit semantic marker.
    pub const fn with_semantics(
        table: TableRef<T>,
        name: &'static str,
        sql_type: SqlType,
        nullability: Nullability,
        semantics: ColumnSemantics,
    ) -> Self {
        Self {
            table,
            name,
            sql_type,
            nullability,
            semantics,
        }
    }

    /// Declaring table.
    pub const fn table(self) -> TableRef<T> {
        self.table
    }

    /// Column name.
    pub const fn name(self) -> &'static str {
        self.name
    }

    /// Declared SQL type.
    pub const fn sql_type(self) -> SqlType {
        self.sql_type
    }

    /// Base schema nullability.
    pub const fn nullability(self) -> Nullability {
        self.nullability
    }

    /// Field-level semantic marker.
    pub const fn semantics(self) -> ColumnSemantics {
        self.semantics
    }

    /// Whether the column is marked as a primary key.
    pub const fn is_primary_key(self) -> bool {
        self.semantics.is_primary_key()
    }

    /// Qualify this column with the table's default binding name.
    pub const fn qualified(self) -> QualifiedColumn<T> {
        self.table.bind().column(self)
    }

    /// Qualify this column with an explicit binding name.
    pub const fn qualified_as(self, binding_name: &'static str) -> QualifiedColumn<T> {
        self.table.alias(binding_name).column(self)
    }
}

impl<T> fmt::Debug for Column<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Column")
            .field("table", &self.table)
            .field("name", &self.name)
            .field("sql_type", &self.sql_type)
            .field("nullability", &self.nullability)
            .field("semantics", &self.semantics)
            .finish()
    }
}

impl<T> PartialEq for Column<T> {
    fn eq(&self, other: &Self) -> bool {
        self.table == other.table
            && self.name == other.name
            && self.sql_type == other.sql_type
            && self.nullability == other.nullability
            && self.semantics == other.semantics
    }
}

impl<T> Eq for Column<T> {}

impl<T> core::hash::Hash for Column<T> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.table.hash(state);
        self.name.hash(state);
        self.sql_type.hash(state);
        self.nullability.hash(state);
        self.semantics.hash(state);
    }
}

impl<T> fmt::Display for Column<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.table, self.name)
    }
}

/// A range binding introduced by `FROM ...` or `JOIN ...`.
pub struct Binding<T> {
    table: TableRef<T>,
    name: &'static str,
    nullability: Nullability,
}

impl<T> Copy for Binding<T> {}

impl<T> Clone for Binding<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Binding<T> {
    const fn new(table: TableRef<T>, name: &'static str, nullability: Nullability) -> Self {
        Self {
            table,
            name,
            nullability,
        }
    }

    /// Bound table.
    pub const fn table(self) -> TableRef<T> {
        self.table
    }

    /// Binding name visible to qualified column references.
    pub const fn name(self) -> &'static str {
        self.name
    }

    /// Binding-level nullability, for example after an outer join.
    pub const fn nullability(self) -> Nullability {
        self.nullability
    }

    /// Whether the binding name differs from the table name.
    pub fn is_alias(self) -> bool {
        self.name != self.table.name()
    }

    /// Replace the binding-level nullability.
    #[must_use]
    pub const fn with_nullability(self, nullability: Nullability) -> Self {
        Self::new(self.table, self.name, nullability)
    }

    /// Mark the binding as nullable.
    #[must_use]
    pub const fn nullable(self) -> Self {
        self.with_nullability(Nullability::Nullable)
    }

    /// Qualify a table column through this binding.
    pub const fn column(self, column: Column<T>) -> QualifiedColumn<T> {
        QualifiedColumn::new(self, column)
    }
}

impl<T> fmt::Debug for Binding<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Binding")
            .field("table", &self.table)
            .field("name", &self.name)
            .field("nullability", &self.nullability)
            .finish()
    }
}

impl<T> PartialEq for Binding<T> {
    fn eq(&self, other: &Self) -> bool {
        self.table == other.table
            && self.name == other.name
            && self.nullability == other.nullability
    }
}

impl<T> Eq for Binding<T> {}

impl<T> core::hash::Hash for Binding<T> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.table.hash(state);
        self.name.hash(state);
        self.nullability.hash(state);
    }
}

impl<T> fmt::Display for Binding<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name)
    }
}

/// A column resolved through an in-scope binding.
pub struct QualifiedColumn<T> {
    binding: Binding<T>,
    column: Column<T>,
}

impl<T> Copy for QualifiedColumn<T> {}

impl<T> Clone for QualifiedColumn<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> QualifiedColumn<T> {
    const fn new(binding: Binding<T>, column: Column<T>) -> Self {
        Self { binding, column }
    }

    /// Active binding.
    pub const fn binding(self) -> Binding<T> {
        self.binding
    }

    /// Underlying schema column.
    pub const fn column(self) -> Column<T> {
        self.column
    }

    /// Bound table.
    pub const fn table(self) -> TableRef<T> {
        self.column.table()
    }

    /// Binding-visible qualifier.
    pub const fn binding_name(self) -> &'static str {
        self.binding.name()
    }

    /// Column name.
    pub const fn column_name(self) -> &'static str {
        self.column.name()
    }

    /// SQL type carried by the schema column.
    pub const fn sql_type(self) -> SqlType {
        self.column.sql_type()
    }

    /// Base column nullability from schema metadata.
    pub const fn base_nullability(self) -> Nullability {
        self.column.nullability()
    }

    /// Effective nullability after binding-level widening.
    pub const fn nullability(self) -> Nullability {
        self.column.nullability().widen(self.binding.nullability())
    }
}

impl<T> fmt::Debug for QualifiedColumn<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QualifiedColumn")
            .field("binding", &self.binding)
            .field("column", &self.column)
            .finish()
    }
}

impl<T> PartialEq for QualifiedColumn<T> {
    fn eq(&self, other: &Self) -> bool {
        self.binding == other.binding && self.column == other.column
    }
}

impl<T> Eq for QualifiedColumn<T> {}

impl<T> core::hash::Hash for QualifiedColumn<T> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.binding.hash(state);
        self.column.hash(state);
    }
}

impl<T> fmt::Display for QualifiedColumn<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.binding, self.column.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    crate::schema! {
        mod authored {
            table public.users {
                id: primary_key(int4),
                name: text,
                deleted_at: nullable(timestamptz),
            },
            table public.posts {
                id: pk(int8),
                author_id: int4,
                title: text,
            },
            table service.widgets {
                id: pk(int4),
                name: text,
                active: bool,
            },
        }
    }

    mod fixture {
        use super::*;

        pub enum Users {}
        pub enum Posts {}

        pub const USERS: TableRef<Users> = TableRef::new(Some("public"), "users");
        pub const POSTS: TableRef<Posts> = TableRef::new(Some("public"), "posts");

        pub const USER_ID: Column<Users> =
            Column::new(USERS, "id", SqlType::INT4, Nullability::NonNull);
        pub const USER_NAME: Column<Users> =
            Column::new(USERS, "name", SqlType::TEXT, Nullability::NonNull);
        pub const USER_DELETED_AT: Column<Users> = Column::new(
            USERS,
            "deleted_at",
            SqlType::TIMESTAMPTZ,
            Nullability::Nullable,
        );

        pub const POST_ID: Column<Posts> =
            Column::new(POSTS, "id", SqlType::INT8, Nullability::NonNull);
        pub const POST_AUTHOR_ID: Column<Posts> =
            Column::new(POSTS, "author_id", SqlType::INT4, Nullability::NonNull);

        pub const USERS_ID: QualifiedColumn<Users> = USER_ID.qualified();
        pub const USERS_NAME: QualifiedColumn<Users> = USER_NAME.qualified();
        pub const USER_ALIAS_ID: QualifiedColumn<Users> = USER_ID.qualified_as("u");
        pub const POSTS_AUTHOR_ID: QualifiedColumn<Posts> = POST_AUTHOR_ID.qualified();
    }

    #[test]
    fn qualification_and_binding_metadata_follow_binding_names() {
        let users = fixture::USERS.bind();
        let alias = fixture::USERS.alias("u");
        let users_id = users.column(fixture::USER_ID);
        let alias_id = alias.column(fixture::USER_ID);

        assert_eq!(users.table(), fixture::USERS);
        assert_eq!(users.name(), "users");
        assert!(!users.is_alias());
        assert_eq!(users_id.binding_name(), "users");
        assert_eq!(users_id.column_name(), "id");
        assert_eq!(users_id.to_string(), "users.id");

        assert_eq!(alias.table(), fixture::USERS);
        assert_eq!(alias.name(), "u");
        assert!(alias.is_alias());
        assert_eq!(alias_id.binding_name(), "u");
        assert_eq!(alias_id.to_string(), "u.id");
    }

    #[test]
    fn sql_type_and_nullability_markers_are_const_friendly() {
        assert_eq!(fixture::USER_ID.sql_type(), SqlType::INT4);
        assert_eq!(fixture::USER_ID.sql_type().oid(), types::INT4);
        assert_eq!(fixture::USER_ID.sql_type().name(), "int4");
        assert!(fixture::USER_ID.sql_type().is_resolved());
        assert_eq!(fixture::USER_ID.semantics(), ColumnSemantics::Ordinary);
        assert!(!fixture::USER_ID.is_primary_key());

        assert_eq!(
            fixture::USER_DELETED_AT.nullability(),
            Nullability::Nullable
        );
        assert_eq!(fixture::USER_ID.nullability(), Nullability::NonNull);

        let widened = fixture::USERS
            .alias("u")
            .nullable()
            .column(fixture::USER_NAME);
        assert_eq!(widened.base_nullability(), Nullability::NonNull);
        assert_eq!(widened.nullability(), Nullability::Nullable);

        assert_eq!(SqlType::GEOMETRY.extension_name(), Some("postgis"));
        assert!(!SqlType::GEOMETRY.is_resolved());
    }

    #[test]
    fn symbol_ergonomics_work_for_const_schema_fixtures() {
        assert_eq!(fixture::USERS.to_string(), "public.users");
        assert_eq!(fixture::USER_ID.to_string(), "public.users.id");
        assert_eq!(
            fixture::USERS_ID,
            fixture::USERS.bind().column(fixture::USER_ID)
        );
        assert_eq!(fixture::USERS_NAME.to_string(), "users.name");
        assert_eq!(fixture::USER_ALIAS_ID.to_string(), "u.id");
        assert_eq!(
            fixture::POSTS_AUTHOR_ID,
            fixture::POSTS.bind().column(fixture::POST_AUTHOR_ID)
        );
        assert_eq!(fixture::POST_ID.sql_type(), SqlType::INT8);
    }

    #[test]
    fn authored_schema_macro_emits_multi_table_symbols() {
        assert_eq!(authored::SCHEMA.tables().len(), 3);
        assert_eq!(authored::users::TABLE.schema_name(), Some("public"));
        assert_eq!(authored::users::TABLE.name(), "users");
        assert_eq!(authored::posts::TABLE.schema_name(), Some("public"));
        assert_eq!(authored::posts::TABLE.name(), "posts");
        assert_eq!(authored::widgets::TABLE.schema_name(), Some("service"));
        assert_eq!(authored::widgets::TABLE.name(), "widgets");

        assert_eq!(authored::users::id().sql_type(), SqlType::INT4);
        assert_eq!(authored::users::name().sql_type(), SqlType::TEXT);
        assert_eq!(
            authored::users::deleted_at().nullability(),
            Nullability::Nullable
        );
        assert_eq!(authored::posts::id().sql_type(), SqlType::INT8);
        assert_eq!(authored::posts::author_id().sql_type(), SqlType::INT4);
        assert_eq!(authored::posts::title().sql_type(), SqlType::TEXT);
        assert_eq!(authored::widgets::id().sql_type(), SqlType::INT4);
        assert_eq!(authored::widgets::name().sql_type(), SqlType::TEXT);
        assert_eq!(authored::widgets::active().sql_type(), SqlType::BOOL);
    }

    #[test]
    fn authored_schema_macro_preserves_definition_metadata() {
        let users = authored::SCHEMA
            .find_table(Some("public"), "users")
            .expect("users table exists");
        assert_eq!(users.columns().len(), 3);
        assert_eq!(users.columns()[0].name(), "id");
        assert!(users.columns()[0].semantics().is_primary_key());
        assert_eq!(users.columns()[1].semantics(), ColumnSemantics::Ordinary);
        assert_eq!(users.columns()[2].nullability(), Nullability::Nullable);

        assert!(authored::users::id().is_primary_key());
        assert!(authored::posts::id().is_primary_key());
        assert!(authored::widgets::id().is_primary_key());
        assert!(!authored::posts::author_id().is_primary_key());

        let rematerialized = users.columns()[0].materialize(authored::users::TABLE);
        assert_eq!(rematerialized, authored::users::id());
    }

    #[test]
    fn authored_schema_symbols_are_reusable_across_bindings() {
        let widgets = authored::widgets::TABLE.bind();
        let alias = authored::widgets::TABLE.alias("w");

        let widget_id = authored::widgets::id();
        let widget_name = authored::widgets::name();

        assert_eq!(widgets.column(widget_id).to_string(), "widgets.id");
        assert_eq!(widgets.column(widget_name).to_string(), "widgets.name");
        assert_eq!(alias.column(widget_id).to_string(), "w.id");
        assert_eq!(alias.column(widget_name).to_string(), "w.name");

        let widgets_table = authored::SCHEMA
            .find_table(Some("service"), "widgets")
            .expect("widgets table exists");
        assert_eq!(widgets_table.columns().len(), 3);
        assert_eq!(widgets_table.columns()[2].name(), "active");
        assert_eq!(widgets_table.columns()[2].sql_type(), SqlType::BOOL);
    }
}
