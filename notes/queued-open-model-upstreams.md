# Queued: open-model upstreams (arc G-B of the ApexRouter garden charter)

*Queued 2026-08-01 by the ApexRouter-RS session. Authority: `ApexRouter-RS/docs/GARDEN.md`
(§5 G-B, §6 G2) — the boundary decision is made there and is not re-litigated here.*

Imaginarium currently speaks one upstream: xAI grok-imagine. The garden charter adds two
lanes, both **Imaginarium-owned protocol** (ApexRouter owns only lifecycle/placement/
tunnels/money — decision G2):

1. **fal.ai queue adapter** — submit → poll/webhook, bearer auth. Serves the open fleet
   (FLUX.2 dev $0.012/MP, Wan 2.6 $0.10/s @720p, Seedream, Kling, …) **and hosts
   `xai/grok-imagine` itself**, so one adapter is a candidate to unify the existing closed
   upstream with the open fleet. Design for that unification before writing two clients.
2. **Local/vast ComfyUI adapter** — ComfyUI-GGUF workflows against a ComfyUI instance whose
   *process* (local card or tunnelled vast box at 127.0.0.1:88xx) ApexRouter supervises.
   Verified co-hab-sized GGUFs, 2026-08-01: FLUX.2-klein-9B Q6 7.9 GB ·
   Qwen-Image-2512 Q4_K_M 13.2 GB · Wan2.2-T2V-A14B Q4_K_M 9.7 GB×2 experts ·
   LTX-2.3-22B distilled Q4_K_M 14.2 GB (16 GB min VRAM, wants generous system RAM).

Not started; waiting on the garden measurement campaign (GARDEN.md §7) whose R2 run
measures thinker-vs-image co-habitation on a 48 GB box.
