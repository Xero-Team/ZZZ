---
title: Privacy Policy
slug: privacy-policy
---

**Last Updated**: September 5, 2026

## Summary

ZZZ is community-maintained, open-source software. It does not create
accounts, collect telemetry, process payments, or send data to a hosted
ZZZ service by default.

- The editor runs locally and stores settings and project data on your
  machine.
- There is no product telemetry to opt out of; telemetry is absent.
- Prompts and code leave your machine only when you configure a provider
  or other network integration yourself.
- ZZZ has no default subprocessors.

Questions and privacy requests belong in this repository's issue tracker:
<https://codeberg.org/ZZZEditor/ZZZ/issues/new>.

## Introduction

This Privacy Policy explains how the ZZZ project ("ZZZ") handles
information when you build or run the software in this repository. It
matches the [Terms](./terms.md). ZZZ does not provide accounts,
subscriptions, trials, payments, hosted AI, or hosted collaboration by
default.

As used here, "personal data" means information relating to an identified
or identifiable individual.

## Data stored on your machine

ZZZ stores editor settings, keymaps, recent projects, workspace state,
and similar files in directories on the computer where you run it. That
data stays local unless you copy it elsewhere.

## Data ZZZ does not collect by default

ZZZ does not, by default:

- Create or require an account
- Collect names, email addresses, usernames, or payment details
- Send telemetry, behavioral logs, crash reports, or system identifiers
- Store prompts, source code, or edit-prediction excerpts on a ZZZ
  server
- Honor or need a hosted opt-out toggle, because collection is absent

Automatic application updates are disabled by default. Language-server,
debug adapter, Prettier, Node, and extension downloads may contact
third-party hosts, including the public Zed marketplace at
`https://api.zed.dev`.

## User-configured services

You may explicitly configure a local or remote language-model provider,
Git remote, language server, MCP server, collaboration endpoint,
extension gallery request, or other integration. Network requests to
those destinations are initiated by your configuration or action.

ZZZ does not host those services on your behalf. Retention, training,
and acceptable-use rules for a destination are governed by that
destination's terms and privacy policy. See
[Third-party terms](./third-party-terms.md).

Extension Gallery browse, install, auto-install, and auto-update
requests may contact the public Zed marketplace at
`https://api.zed.dev`. That traffic is not a ZZZ account or telemetry
channel.

## Contact

Report bugs or propose changes at
<https://codeberg.org/ZZZEditor/ZZZ/issues/new>.
