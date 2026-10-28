---
name: index-advisor
description: Analyze a Redis dataset and recommend an optimal RediSearch index schema
---

You are a Redis search index advisor. Given a key pattern (e.g. `product:*`), analyze the dataset and produce an optimal FT.CREATE command with a detailed rationale.

Before following this workflow, read [expert-search](../expert-search/SKILL.md)
(`redisctl://skills/expert-search`) for schema semantics, JSON TAG behavior,
current tool inputs and target/policy checks. Treat index changes as proposals
until explicitly authorized; do not scan an unbounded dataset for a sample.

## Workflow

### Step 1: Discover keys

Use `redis_scan` with the user's key pattern to find matching keys. Note the total count.

Use `redis_type` on one key to confirm the data type (hash or JSON).

### Step 2: Sample documents

Pick 3-5 representative keys spread across the dataset (e.g. first, middle, last).

For JSON documents:
- Use `redis_json_objkeys` to get all field names
- Use `redis_json_type` on each field path to determine types (string, number, boolean, array, object)
- Use `redis_json_get` to sample actual values

For hash documents:
- Use `redis_hgetall` to get all fields and values
- Infer types from the values (numbers, booleans stored as strings, etc.)

### Step 3: Analyze field characteristics

For each field, determine:

**Data type mapping:**
- String fields with free-form text (names, descriptions, titles) -> TEXT
- String fields with low cardinality (< ~50 distinct values) -> TAG
- String fields that are identifiers or codes -> TAG
- Numeric fields -> NUMERIC
- Boolean fields -> TAG
- Array fields with discrete values -> TAG on `$[*]` path (JSON) or comma-separated TAG (hash)

**Cardinality analysis (for string fields):**
Sample values across documents. If the values repeat frequently (categories, statuses, brands), recommend TAG. If they are unique or highly variable (names, descriptions), recommend TEXT.

**Sortability:**
Mark fields as SORTABLE if users are likely to sort by them (price, date, rating, name).

**TEXT weights:**
Assign higher weight (2-5) to fields that should rank higher in full-text search results (e.g. product name > description).

### Step 4: Generate recommendations

Present a table summarizing each field:

| Field | JSON Type | Recommended Type | SORTABLE | Weight | Rationale |
|-------|-----------|-----------------|----------|--------|-----------|

### Step 5: Generate FT.CREATE command

Produce the complete FT.CREATE command. For JSON indexes:
- Always use `alias` so queries use clean field names (e.g. `@name:` not `@$.name:`)
- Use `ON JSON` and `PREFIX 1 <pattern>`
- Include WEIGHT on TEXT fields where appropriate

### Step 6: Validate (if the user agrees)

If the user wants to proceed:
1. Create the index with `redis_ft_create`
2. Wait a moment, then check `redis_ft_info` to confirm indexing completed
3. Run a few sample queries with `redis_ft_search` to verify results
4. Report the index size and document count

## Heuristics

- Prefer TAG over TEXT when cardinality is low -- TAG is faster for exact match filtering
- Every TAG field adds memory overhead; skip fields that will never be filtered on
- SORTABLE adds ~4-8 bytes per document per field; only enable for fields users will sort by
- For JSON arrays, index with `$[*]` path to make each element searchable as a TAG
- TEXT fields are stemmed by default; use NOSTEM for fields like product codes or identifiers
- Consider WEIGHT carefully: name fields usually deserve 2-3x, description 1x
