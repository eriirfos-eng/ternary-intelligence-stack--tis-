# Ternlang Licensing Guide

Ternlang uses a **three-tier licensing model** to balance open-source accessibility with commercial sustainability.

## TL;DR

| Your Scenario | License | Cost | Action |
|---|---|---|---|
| **Personal use / learning** | LGPL-3.0 or BSL-1.1 | Free | ✅ Go ahead |
| **Academic research** | LGPL-3.0 or BSL-1.1 | Free | ✅ Cite [whitepaper](https://doi.org/10.17605/OSF.IO/TZ7DC) |
| **Commercial SaaS** | BSL-1.1 | License required | ❌ Contact licensing@ternlang.com |
| **Closed-source proprietary** | BSL-1.1 | License required | ❌ Contact licensing@ternlang.com |
| **Training ML models** | Any tier | Forbidden | ❌ Not permitted |

## Tier 1: Open Core (LGPL-3.0)

**What:** Language, compiler, VM, LSP, package manager  
**Crates:** `ternlang-core`, `ternlang-cli`, `ternlang-lsp`, `ternlang-compat`, `ternpkg`  
**License:** GNU Lesser General Public License v3.0

### What you can do:
- ✅ Use for any purpose
- ✅ Modify source code
- ✅ Redistribute (with source)
- ✅ Use in SaaS/proprietary software*
- ✅ Write `.tern` programs freely

### What you must do:
- 📋 Provide source code (or build instructions)
- 📋 Include LGPL-3.0 license text
- 📋 Document changes to the crate

**\* Caveat:** LGPL allows "dynamic linking" workarounds for proprietary use. Consult a lawyer if you need a license for certainty.

### Example: Using ternlang-cli in your project

```bash
# Legal: You can distribute a link to build instructions
Your Binary (proprietary) ←→ ternlang-cli (LGPL-3.0, dynamic link)

# Or: Contribute back changes
Your changes → https://github.com/eriirfos-eng/ternary-intelligence-stack
```

---

## Tier 2: Restricted (Business Source License 1.1)

**What:** ML kernels, MoE orchestrator, API, MCP server, HDL, runtime  
**Crates:** `ternlang-ml`, `ternlang-moe`, `ternlang-api`, `ternlang-mcp`, `ternlang-hdl`, `ternlang-runtime`  
**License:** Business Source License 1.1 (BSL-1.1)  
**Auto-conversion:** Apache-2.0 on 2030-04-03 (4 years from release)

### What you can do:
- ✅ Use for personal projects
- ✅ Use for non-commercial academic research
- ✅ View and modify source code
- ✅ Contribute changes back (optional)

### What you cannot do (without a license):
- ❌ Use in commercial SaaS
- ❌ Ship in proprietary applications
- ❌ Provide as a service to paying customers
- ❌ Use in products where you generate revenue

### Getting a Commercial License

**Email:** licensing@ternlang.com

**Include:**
1. **Company/project name**
2. **Intended use:** SaaS, mobile app, embedded system, etc.
3. **Deployment:** How many users/instances?
4. **Timeline:** When do you need it operational?
5. **Budget:** What's your budget range?

**Typical license:**
- €24–€5,000/month depending on use case and scale
- Negotiable for non-profit / startup / academic partnerships

### Free vs. Licensed Endpoints (Hosted API)

**Free tier (`https://ternlang.com/mcp`):**
- 10 MCP tools, no authentication required
- Suitable for: prototyping, research, hobby projects
- Rate limit: Reasonable use

**Licensed tier (REST API, `X-Ternlang-Key`):**
- 10 premium tools + 8 REST endpoints + 2 SSE streams
- 10,000 calls/month (Tier 2: €24/month)
- Suitable for: production SaaS, commercial deployments

### Auto-Conversion to Apache-2.0 (2030-04-03)

On **April 3, 2030**, all Tier 2 crates automatically convert to **Apache-2.0**. After that date:
- ✅ Unrestricted commercial use
- ✅ No license required
- ✅ Modifications must include attribution

---

## Tier 3: Proprietary (Hosted Services)

**What:** ternlang.com infrastructure, enterprise SLA, custom inference engines  
**License:** Proprietary (contact licensing@ternlang.com)

### What's included:
- Commercial API key management
- Enterprise SLA (uptime guarantees)
- Priority support
- Custom feature requests

---

## ML Training Restriction

### ⚠️ CRITICAL

**The contents of this repository may not be used to train, fine-tune, or distill machine learning models without explicit written permission from RFI-IRFOS.**

This applies to:
- ✋ All Rust source code
- ✋ All `.tern` example programs
- ✋ Documentation and guides
- ✋ Whitepaper and specifications
- ✋ Pre-trained weights (if any)

**Exception:** Your own `.tern` programs written *against* the ternlang platform are yours to use freely (including for training).

### Why this restriction exists

Ternlang is a research framework with novel balanced ternary algorithms and architectural insights. This restriction protects the intellectual property and ensures proper attribution in derivative research.

---

## License Compatibility

### Can I use Ternlang with...

| Library | LGPL-3.0 | BSL-1.1 | Notes |
|---------|----------|---------|-------|
| Apache-2.0 | ✅ | ✅ | Compatible |
| MIT | ✅ | ✅ | Compatible |
| GPL-2.0 | ❌ | ✅* | Tier 1 is incompatible; Tier 2 can coexist |
| AGPL-3.0 | ⚠️ | ✅* | Network clause may trigger; consult lawyer |
| Proprietary | ✅* | ❌ | Tier 1 with dynamic linking only; Tier 2 needs license |

**\* Consult a lawyer for your specific use case.**

---

## Decision Trees

### "Can I use this in my project?"

```
Q: Are you using Tier 1 crates only?
   YES → Can you provide source / build instructions?
         YES → ✅ Legal for commercial use
         NO  → ❌ License needed (OR use dynamic linking workaround)
   NO → Is your project commercial?
        YES → 💰 License required → email licensing@ternlang.com
        NO  → ✅ Legal for personal/academic use
```

### "Am I allowed to train ML models?"

```
Q: Using Ternlang source code / examples?
   YES → ❌ NOT PERMITTED (any tier)
   NO  → Are you using pre-trained weights?
         YES → ❌ NOT PERMITTED (likely)
         NO  → ✅ Your own models are fine
```

### "When can I use BSL-1.1 code commercially?"

```
Q: Is it before April 3, 2030?
   YES → Need a license → licensing@ternlang.com
   NO  → Apache-2.0 applies automatically → ✅ No license needed
```

---

## FAQ

**Q: Can I fork Ternlang and build a commercial product?**

A: Depends on which crates:
- **Tier 1 (LGPL-3.0):** Technically yes, but you must provide source/build instructions. Consult a lawyer.
- **Tier 2 (BSL-1.1):** No, you need a commercial license. Email licensing@ternlang.com.

**Q: Do I need to pay for personal use?**

A: No. LGPL-3.0 and BSL-1.1 are both free for personal and academic use.

**Q: What if I modify a Tier 2 crate?**

A: You can modify and use for personal use. If you want to share modifications, ideally contribute back to the main repo. For commercial use of modifications, you still need a license.

**Q: Is the hosted API ($24/month) the only commercial option?**

A: No. You can also:
1. License the source code and run it yourself (on-premises)
2. Use the free MCP server (https://ternlang.com/mcp) for development

**Q: Does the auto-conversion to Apache-2.0 in 2030 mean Ternlang is becoming open?**

A: Yes. On 2030-04-03, all Tier 2 crates become fully open under Apache-2.0. This is a deliberate design: source code is visible now, but commercial use is restricted until the auto-conversion date.

**Q: Can I use Ternlang to build a competing product?**

A: For Tier 1 (LGPL-3.0): Yes, with source/build instructions.  
For Tier 2 (BSL-1.1): Not without a license.  
Contact licensing@ternlang.com if you'd like to discuss.

---

## Contact

| | |
|---|---|
| **Licensing questions** | licensing@ternlang.com |
| **Technical support** | community@ternlang.com |
| **Security issues** | security@ternlang.com |
| **Website** | [ternlang.com](https://ternlang.com) |

---

*Last updated: 2026-04-05*