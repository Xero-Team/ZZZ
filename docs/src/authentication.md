---
title: Authenticate with ZZZ
description: "Sign in to ZZZ to access account-backed and hosted services."
---

Signing in to ZZZ is not required. You can use most features you'd expect in a code editor without ever doing so. We'll outline the few features that do require signing in, and how to do so, here.

## What Features Require Signing In?

The main in-editor reason to sign in is to use [LLM-powered features](./ai/overview.md) when you are using ZZZ as the provider of your LLM models. To use AI without signing in, you can [bring and configure your own API keys](./ai/llm-providers.md#use-your-own-keys).

## Signing In

ZZZ uses GitHub's OAuth flow to authenticate users, requiring only the `read:user` GitHub scope, which grants read-only access to your GitHub profile information.

1. Open the command palette and run the `client: sign in` command (`cmd-shift-p` on macOS or `ctrl-shift-p` on Windows/Linux).
2. Your default web browser will open to the ZZZ sign-in page.
3. Authenticate with your GitHub account when prompted.
4. After successful authentication, your browser will display a confirmation, and you'll be automatically signed in to ZZZ.

**Note**: If you're behind a corporate firewall, ensure that connections to `zed.dev` are allowed.

## Signing Out

To sign out of ZZZ, you can use either of these methods:

- Click on the profile icon in the upper right corner and select `Sign Out` from the dropdown menu.
- Open the command palette and run the `client: sign out` command.

## Email Addresses {#email}

Your ZZZ account's email address is the address provided by GitHub OAuth. If you have a public email address then it will be used, otherwise your primary GitHub email address will be used. Changes to your email address on GitHub can be synced to your ZZZ account by [signing in to zed.dev](https://zed.dev/sign_in).

Stripe is used for billing, and will use your ZZZ account's email address when starting a subscription. Changes to your ZZZ account email address do not currently update the email address used in Stripe. See [Updating Billing Information](./ai/billing.md#updating-billing-info) for how to change this email address.
