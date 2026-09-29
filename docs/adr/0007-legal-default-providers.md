# 0007. Ship only legal sources as default providers

- **Status:** Accepted
- **Date:** 2026-09-28

## Context

App stores remove apps built around piracy, and bundling piracy-oriented indexers creates legal exposure for the project.

## Decision

The app ships with only **legal sources** enabled (Internet Archive, Academic Torrents, Linux distributions, public-domain collections). The first-party definitions repo contains only legal sources. Any other indexer is added by the user: a Torznab endpoint, an imported definition, or a third-party repo.

## Consequences

- Hashlark is a neutral search tool, which gives the best chance of store approval.
- Out of the box, results are limited until the user adds providers. The first-run flow has to make adding providers easy.
- Community repos can be maintained independently of the project.
