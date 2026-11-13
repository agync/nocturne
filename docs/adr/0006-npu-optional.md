---
type: Decision
title: "0006 The NPU is optional"
description: "The NPU works only when the GPU is idle, with a GPU fallback."
status: draft
tags: [adr]
---

# 0006 The NPU is optional

## Decision

Salience classification uses the NPU when the GPU is idle; otherwise it runs on the GPU. The default
path is sequential. Concurrent NPU and GPU use is an experiment.

## Why

One measurement showed no contention, but a later report records an NPU plus GPU crash. The product
cannot depend on an unstable path.
