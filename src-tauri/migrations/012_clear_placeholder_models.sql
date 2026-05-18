-- Remove legacy hardcoded placeholder models from existing configs
UPDATE provider_configs 
SET config = json_set(config, '$.model', '') 
WHERE json_extract(config, '$.model') IN ('meta-llama/llama-4-maverick', 'unknown');
