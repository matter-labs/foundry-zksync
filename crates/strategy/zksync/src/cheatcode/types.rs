/// Setting for migrating the database to zkEVM storage when starting in ZKsync mode.
/// The migration is performed on the DB via the inspector so must only be performed once.
#[derive(Debug, Default, Clone)]
pub enum ZkStartupMigration {
    /// Defer database migration to a later execution point.
    ///
    /// This is required as we need to wait for some baseline deployments
    /// to occur before the test/script execution is performed.
    #[default]
    Defer,
    /// Allow database migration.
    Allow,
    /// Database migration has already been performed.
    Done,
}

impl ZkStartupMigration {
    /// Check if startup migration is allowed. Migration is disallowed if it's to be deferred or has
    /// already been performed.
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow)
    }

    /// Allow migrating the the DB to zkEVM storage.
    pub fn allow(&mut self) {
        *self = Self::Allow
    }

    /// Mark the migration as completed. It must not be performed again.
    pub fn done(&mut self) {
        *self = Self::Done
    }
}
