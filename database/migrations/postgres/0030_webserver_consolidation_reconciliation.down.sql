-- Rollback of 0030_webserver_consolidation_reconciliation. The reconciliation
-- only ever ADDS objects that the consolidated baseline already declares, so
-- the down migration is a no-op placeholder: dropping the table here would
-- destroy data the baseline still requires.
SELECT 1;
