-- 022: Support SUB_WORKFLOW nodes in workflow graph.
--
-- Enables nested / sub-workflow execution where a node invokes another workflow,
-- waits for its completion, and folds its outputs into the parent workflow context.

ALTER TABLE workflow_nodes
    DROP CONSTRAINT IF EXISTS workflow_nodes_type_check;

ALTER TABLE workflow_nodes
    ADD CONSTRAINT workflow_nodes_type_check
    CHECK (node_type IN ('JOB', 'APPROVAL', 'DELAY', 'CONDITION', 'MAP', 'WEBHOOK', 'SUB_WORKFLOW'));
