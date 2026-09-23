-- Frozen workspace-13 protocol gate for durable native admission.
-- Business tables are unchanged; old schema-12 hosts must refuse a new open.
UPDATE metadata SET value='13' WHERE key='schema_version' AND value='12';
