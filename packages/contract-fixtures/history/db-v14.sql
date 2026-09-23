-- Frozen workspace-14 protocol gate for media, plugin and download admission.
-- Business tables are unchanged; schema-13 hosts must refuse a new open.
UPDATE metadata SET value='14' WHERE key='schema_version' AND value='13';
