---
name: Security Review
description: Scan for OWASP Top 10 using grep patterns. Report vulnerabilities with file:line.
type: active
match: 
---
# Security Audit — Tool Workflow

Run these grep patterns on the workspace. Report every hit:

Step 1: Injection detection:
  `grep "execute\|execSQL\|rawQuery" --glob "*.kt"`
  `grep "ProcessBuilder\|Runtime\.exec" --glob "*.kt"`
  `grep "innerHTML\|document\.write" --glob "*.html"`

Step 2: Secrets detection:
  `grep "password\|secret\|api_key\|token\s*=" --glob "*"`
  `grep "BEGIN.*PRIVATE KEY" --glob "*"`

Step 3: Crypto weakness:
  `grep "MD5\|SHA-1\|DES\|RC4" --glob "*"`

Step 4: Auth issues:
  `grep "\.isAuthenticated\|\.hasRole\|checkPermission" --glob "*.kt"`

Step 5: Report each finding with file:line + OWASP category + fix.
Don't just list what to check. Actually run the grep commands.
