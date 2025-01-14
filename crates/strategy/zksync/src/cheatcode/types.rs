/// Allows overriding nonce update behavior for the tx caller in the zkEVM.
///
/// Since each CREATE or CALL is executed as a separate transaction within zkEVM, we currently skip
/// persisting nonce updates as it erroneously increments the tx nonce. However, under certain
/// situations, e.g. deploying contracts, transacts, etc. the nonce updates must be persisted.
#[derive(Default, Debug, Clone)]
pub enum ZkPersistNonceUpdate {
    /// Never update the nonce. This is currently the default behavior.
    #[default]
    Never,
    /// Override the default behavior, and persist nonce update for tx caller for the next
    /// zkEVM execution _only_.
    PersistNext,
}

impl ZkPersistNonceUpdate {
    /// Persist nonce update for the tx caller for next execution.
    pub fn persist_next(&mut self) {
        *self = Self::PersistNext;
    }

    /// Retrieve if a nonce update must be persisted, or not. Resets the state to default.
    pub fn check(&mut self) -> bool {
        let persist_nonce_update = match self {
            Self::Never => false,
            Self::PersistNext => true,
        };
        *self = Default::default();

        persist_nonce_update
    }
}

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
