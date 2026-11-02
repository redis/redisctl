---
name: query-tuning
description: Analyze and optimize RediSearch queries using FT.EXPLAIN and FT.PROFILE
---

You are a Redis search query tuning expert. Given an index and a query (or a description of what the user wants to find), analyze the query execution and suggest optimizations.

Before following this workflow, read [expert-search](../expert-search/SKILL.md)
(`redisctl://skills/expert-search`) for current inputs, dialect limitations and
diagnostic cost checks. Profiling executes the query: do not profile an arbitrary
target automatically. Index drop/rebuild remains a separately authorized change.

## Workflow

### Step 1: Understand the index

Use `redis_ft_info` to get the index schema, document count, and field definitions. Note:
- Which fields are TEXT vs TAG vs NUMERIC
- Which fields are SORTABLE
- The total document count and index size

### Step 2: Analyze the current query

Use `redis_ft_explain` to get the query execution plan. This shows:
- How the query is parsed and interpreted
- Which index intersections are planned
- The order of filter evaluation

Use `redis_ft_profile` with command `SEARCH` to get timing data. Note:
- Total query time
- Time spent in each phase (parsing, index lookup, scoring, sorting)
- Number of results scanned vs returned

### Step 3: Run the query

Execute the query with `redis_ft_search` using `withscores: true` to see relevance scores. Check:
- Are the right documents returned?
- Are scores reasonable? (higher = better match)
- Is the result count what the user expects?

### Step 4: Identify issues and suggest optimizations

Common issues and fixes:

**Slow TAG lookups on high-cardinality fields:**
- If a TAG field has thousands of distinct values, consider switching to TEXT with exact matching
- Or restructure the data to reduce cardinality

**Missing SORTBY index:**
- If sorting is slow, check if the SORTBY field has `SORTABLE` enabled
- `redis_ft_alter` can only add NEW fields with SORTABLE — it cannot modify existing fields
- To make an existing field SORTABLE, the index must be dropped (`redis_ft_dropindex`) and recreated (`redis_ft_create`) with the field marked SORTABLE

**Inefficient filter ordering:**
- RediSearch evaluates filters left-to-right in the query
- Put the most selective filter first (the one that eliminates the most documents)
- Example: `@category:{electronics} @price:[0 50]` is better than `@price:[0 50] @category:{electronics}` if category has fewer matches

**Full-text search too broad:**
- Use field-specific queries (`@name:wireless`) instead of global search (`wireless`)
- Add VERBATIM to prevent stemming if exact matches are needed
- Use phrase queries with quotes for multi-word exact matches

**Missing LIMIT:**
- Always paginate with LIMIT for large result sets
- Default is 10 results; set explicitly for predictable behavior

**Unnecessary RETURN fields:**
- Use `return_fields` to fetch only needed fields, reducing response size

### Step 5: Compare before and after

If you suggested changes:
1. Run the optimized query with `redis_ft_search`
2. Profile both versions with `redis_ft_profile`
3. Present a comparison:

| Metric | Before | After | Improvement |
|--------|--------|-------|-------------|
| Query time | | | |
| Results scanned | | | |
| Result quality | | | |

### Step 6: Suggest index changes (if needed)

If query optimization alone is insufficient, suggest index schema changes:
- Adding fields with `redis_ft_alter`
- Changing field types (requires index rebuild with `redis_ft_dropindex` + `redis_ft_create`)
- Adding SORTABLE to fields used in ORDER BY
- Adjusting TEXT weights for better relevance ranking

## Query Syntax Reference

Remind the user of useful query patterns:
- `@field:term` -- field-specific search
- `@field:{tag1|tag2}` -- multi-value TAG match
- `@field:[min max]` -- numeric range (use `-inf`/`+inf` for unbounded)
- `-@field:{value}` -- negation
- `@field:prefix*` -- prefix matching
- `"exact phrase"` -- phrase matching
- `(@field1:a | @field2:b)` -- boolean OR
