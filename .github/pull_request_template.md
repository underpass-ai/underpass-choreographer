## Summary

Describe the change briefly.

## Why This Belongs In The Choreographer

Explain why this is choreographer-owned work and not integrating-product logic.
Remember the Choreographer is:

- **use-case agnostic** (no SWE, clinical, supply-chain, … vocabulary)
- **provider-agnostic** (no vLLM, Anthropic, OpenAI, … privileged)
- **API-first** (proto + AsyncAPI are the contract)

## Checks

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings`
- [ ] `cargo test --workspace --locked`
- [ ] `bash scripts/ci/contract-gate.sh`
- [ ] `bash scripts/ci/quality-gate.sh`
- [ ] Unit coverage ≥ 80 % (target band 80–90 %)

## Contract Impact

- [ ] No public contract changes
- [ ] gRPC (`underpass.choreo.v1`) contract changed
- [ ] AsyncAPI (`specs/asyncapi/choreographer.asyncapi.yaml`) changed
- [ ] Helm chart public surface changed

## Honesty & Evidence

- [ ] No claim in this PR (code, docs, commit message) is unsubstantiated.
- [ ] Every behavioural claim has a test; every performance claim has a
      benchmark committed under `docs/experiments/`; every quality claim
      has a CI gate.
- [ ] If behaviour changed non-trivially, an entry under
      `docs/experiments/NNN-…/` records hypothesis → method → results
      → conclusion → threats to validity.
- [ ] No marketing language ("blazingly", "industry-leading", "99.x %")
      without a linked measurement in this repo.

## Architecture Review

- [ ] No god object introduced
- [ ] No god file introduced
- [ ] DDD and hexagonal boundaries preserved
- [ ] No primitive obsession in domain APIs (newtypes / value objects)
- [ ] SOLID respected (SRP, DIP, ISP in particular)
- [ ] No integrating-product nouns added to the choreographer boundary
- [ ] No LLM-provider identity leaks into core
- [ ] Docs updated where needed
