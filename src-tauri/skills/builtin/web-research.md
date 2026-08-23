---
name: Web Research
description: Web search, page fetch, URL analysis, information retrieval
type: active
match: [联网搜索, 网页搜索, 上网查, web research, search the web]
---

# Web Research

Rules for finding and verifying information online.

Tool routing:
- Unknown source or broad question: `web_search`.
- Known public URL: `web_fetch`.
- Browser only for explicit visual interaction, JavaScript, login, clicking, or form work.
- Reuse URLs already returned; never repeat the same search unchanged.

## 1. Search with intent, not keywords

Formulate search queries as complete questions or phrases, not keyword lists:
```
# Bad: web_search "python async best practice"
# Good: web_search "Python asyncio best practices for HTTP clients 2025"
```

Use different phrasings if the first query returns irrelevant results. Try at most
3 query variations before concluding the information is not publicly available.

Failure avoided: returning "no results found" when better phrasing would find them.

## 2. Fetch and verify sources

After `web_search` returns URLs:
1. Use `web_fetch` to read 2-3 most relevant pages
2. Cross-check claims between sources — single-source claims are flagged
3. Check publication date — prefer sources from the last 2 years
4. Skip SEO spam, link farms, and AI-generated content farms

Failure avoided: citing outdated or hallucinated information.

## 3. Cite with URLs

Every factual claim from web sources must include the source URL:
```
# Bad: "Python 3.13 added a new GIL" (no source)
# Good: "Python 3.13 introduced free-threaded mode (PEP 703) — https://peps.python.org/pep-0703/"
```

Failure avoided: user cannot verify or follow up on cited claims.

## 4. Know when to stop

Stop searching if:
- 2 `web_search` calls return no relevant results → use existing knowledge
- 3+ pages all agree on the answer → sufficient
- Page content is behind a paywall → report the URL and note the limitation

Do not spend more than 5 tool calls on a single search task.
