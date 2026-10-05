-- 025: an explicit JOIN workflow node.
--
-- `redesign.md` §2.C lists "fan-out and fan-in" as a required node type. Fan-in
-- already worked through edge conditions, but a join carries its own barrier
-- policy, so it needs a value of its own rather than being spelled as a set of
-- edge conditions.

ALTER TABLE workflow_nodes
    DROP CONSTRAINT IF EXISTS workflow_nodes_type_check;

ALTER TABLE workflow_nodes
    ADD CONSTRAINT workflow_nodes_type_check
    CHECK (node_type IN (
        'JOB', 'APPROVAL', 'DELAY', 'CONDITION', 'MAP',
        'WEBHOOK', 'SUB_WORKFLOW', 'JOIN'
    ));
