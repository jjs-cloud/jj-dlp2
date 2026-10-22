# Stream Recorder Engine: Brief and Kickoff Prompt

How to use this file:

- **First chat:** paste Part A and Part B together.
- **Every later chat:** paste Part A, then say which step you are on. Claude has no memory between chats, so Part A is the memory.
- **Before the first chat:** fill in the two `[FILL IN]` blocks in Part C. Keep them short and written as behavior, not code.
- **Keep Part A current.** When you approve a decision in a chat, add it to the "Decisions log" at the bottom of Part A.

---

# PART A: Project brief (paste into every chat)

## A1. What we are building

A **monitoring and recording engine** for live streams, written in Rust, for **Windows and Linux**. It watches a list of streaming channels, notices when one goes live, records it by running an external yt-dlp executable, and reports everything that happens.

It is a **foundation**. Many features will be built on top of it later by separate layers. The engine itself stays small, strict, and extremely stable.

This is a **clean room design**. Design from this brief only. Do not ask for, and do not assume knowledge of, any earlier implementation. If a behavior is needed, ask the owner to describe the behavior.

## A2. Priorities, in order

1. **Stability.** Live streams are unpredictable, so the core must never be the unstable part. A crash, hang, or slow consumer in any layer above must not affect an active recording.
2. **No orphaned processes.** See A6. This is the most important requirement.
3. **Flexibility at the boundary.** The core has a small, stable API. Flexibility comes from layers built on that API and from two swappable components, not from configuration options inside the core.
4. **Simplicity.** Prefer the smallest design that meets the above. Do not add features, options, or abstractions that were not approved.

## A3. Architecture

Three tiers. Dependencies point downward only.

```
 Consumers (GUI, CLI, file tools, notifiers, ...)   <- later, separate
 Policy layers (retry/recovery, scheduling, ...)    <- later, separate
 Default policy (minimal)                           <- part of v1
 ------------------------------------------------------------------
 Engine core (mechanisms only)                      <- the focus now
```

Rules:

- **The engine owns all state.** Everyone else gets read-only views and changes things only by sending commands.
- **Mechanisms in the core, decisions in policy.** The core can start, stop, and observe. It does not decide when to retry, what quality to pick, or when to give up. It publishes facts and obeys commands.
- **Plain data at the boundary.** Commands, events, and snapshots are plain owned data (no references into engine internals, no callbacks that run inside the engine, no shared locks). This lets the same API later be wrapped in IPC or an HTTP API without redesign.
- **No hooks that run inside the core.** Layers react to events and send commands. They never inject code that the engine calls.

## A4. The boundary (public API)

The API is small. It is designed to be frozen. Internals may change freely.

**Commands (in):** a minimal set, for example: add target, remove target, start recording, stop recording (graceful or force), shutdown. The exact list is proposed by Claude and approved by the owner.

**Events (out):** an ordered stream of facts, for example: target went live, target went offline, recording starting, recording started, recording output line, recording finished (with an outcome), detection failed, engine shutting down. Event enums must be **extensible** (`#[non_exhaustive]`), so later versions can add variants without breaking consumers.

**Snapshot (out):** an immutable view of everything the engine currently knows (targets, their live status, active recordings, file paths being written). Consumers can read it as often as they like, for example once per GUI frame, without blocking the engine.

**Delivery rules:**

- A slow or dead consumer must **never** block or slow the engine. Use bounded buffers that drop the oldest data when full, and tell the consumer that data was dropped.
- Every recording must end with exactly one terminal "finished" event that states the outcome (exited with code, failed to spawn, stopped by command, killed by engine, and so on), including when something goes wrong.
- The engine reports a file as finished only after the recorder process has fully exited and been reaped.

## A5. The two swappable components

Only these two are traits. Everything else is concrete.

- **`Detector`**: answers "is this target live right now?" The first implementation shells out to the yt-dlp executable (see A7).
- **`Recorder`**: starts and supervises one recording for one target. The first implementation runs the yt-dlp executable as a child process.

Other things are deliberately not swappable. Output naming, per-target settings, and settings inheritance belong to a config layer above the engine. The engine receives a **fully resolved** recording spec with each start command: the URL, output location, an opaque output template string, and an opaque list of extra arguments. The engine passes these through without interpreting them.

A tiny fake `Recorder` (for example, one that runs a harmless long-lived command) should exist so the engine and policy can be tested without a live stream. It is not a simulation framework.

## A6. Process hygiene (non-negotiable)

**No process started by the engine may ever outlive the engine.** This includes the yt-dlp process and anything yt-dlp itself spawns (such as ffmpeg). It must hold in all of these cases:

- normal shutdown
- the engine panics
- the engine is killed abruptly (task manager "End task", `kill -9`)
- the GUI or parent program crashes

Required behavior:

- **Graceful stop first.** Stopping a recording first sends the platform's polite interrupt so the recorder can finish and close its file. Escalate to force-kill after a timeout.
- **Kill the whole tree**, not just the direct child.
- **Windows:** use a Job Object with kill-on-close semantics (or an equivalent that covers the whole tree).
- **Linux:** Claude must design and justify a mechanism that covers the child and its descendants even if the engine is killed abruptly. Claude should consider process groups/sessions, `PR_SET_PDEATHSIG` (including its known caveat that it fires when the spawning *thread* exits, not the process, and that it does not by itself reach grandchildren), and a watchdog approach. Pick one, and say why.
- **Proof, not trust.** This must be verified with a test on each OS: start a long-running child, kill the engine process abruptly, then confirm the child and its descendants are gone.
- Every child process is always reaped (no zombies) and every pipe is always drained (no deadlocks from full pipe buffers).

This is the riskiest part of the project, so it is built and proven first.

## A7. External tools

- The engine runs a **forked yt-dlp executable**: `yt-dlp.exe` on Windows, `yt-dlp` on Linux. It is not stock yt-dlp, so do not assume stock behavior or output formats beyond what the owner confirms.
- The executable path is supplied to the engine. The engine does not download, update, or bundle it. It must not assume Python is installed.
- ffmpeg is invoked by yt-dlp, not by the engine, so the engine does not call it directly.
- When Claude needs the exact detection or recording command line, or what the output looks like, it must **ask the owner to run the command and paste the real output**. Claude must not guess flags or output formats.

## A8. Stability rules for all core code

- No `unwrap()`, `expect()`, or indexing that can panic in the core. Errors are typed and returned.
- Each recording is supervised in isolation. A failure in one cannot affect another, and a panic is contained to that recording and reported as a failure event.
- Every external call (spawn, wait, read, detection) has a timeout or is otherwise unable to hang the engine forever.
- No unbounded queues, no unbounded growth in memory or log buffers.
- No consumer-supplied code ever runs inside the engine's own threads or tasks.
- Logging uses structured logging. Raw recorder stdout and stderr are available per recording as optional events, bounded as above.

## A9. Out of scope for v1, but design so they can be added later

Do **not** build any of these now. Just do not make them hard to add.

- Recovery behavior (what to do when a recording ends unexpectedly)
- Detecting unhealthy recordings (for example, stalls or files that stop growing). The `Recorder` trait and event enums should leave room for this.
- Scheduling and time-based rules
- Per-target and per-site settings with inheritance
- Output post-processing (moving, converting, trimming files)
- Notifications
- Persistence of state across restarts (a layer above will do this)
- Multi-site support beyond what the owner confirms
- A GUI (a thin GUI comes after the engine is solid)

## A10. Deliverable shape for v1

A Cargo workspace:

- `engine`: library crate, no GUI dependencies. The core, the two traits, the yt-dlp implementations, the fake recorder.
- `default_policy`: library crate. A minimal policy: when a target is live and not being recorded, start a recording. Nothing else.
- `cli`: a small headless binary that wires the two together and prints events, so the engine can be run and tested without any GUI.

Must build and run on Windows and Linux. Claude proposes the concurrency model (plain threads versus async) and justifies it. Bias toward the simplest option that satisfies A6 and A8.

## A11. Decisions log

Add approved decisions here as the project progresses.

- (empty)

---

# PART B: Kickoff prompt (first chat only)

> Paste Part A above this, then paste everything below.

You are helping me design and build the engine described in Part A, in Rust, one small approved step at a time. I am on the **free plan with low usage limits**, so work in small steps, and do not waste output.

## How we work

1. **One step at a time.** Do exactly one step, then stop and wait for my approval. Never continue to the next step on your own.
2. **Small steps.** Aim for roughly 150 lines of new code or less per step. If a step is bigger, split it and tell me.
3. **Design before code.** For each step, first give a short plan (what you will do, which files, what you are not doing). Wait for me to approve it. Only then write code.
4. **Nothing unapproved.** Do not add features, options, dependencies, or abstractions I have not approved. If you think something is needed, propose it and ask. Ask before adding any crate.
5. **Tell me how to test.** End every code step with the exact commands to run on Windows and on Linux, and what output I should expect. I will paste back the results or any compiler errors.
6. **Ask, don't guess.** If you need a real command line, real yt-dlp output, or a decision, ask me. Ask at most three questions at a time.
7. **Comments in code:** one or two lines, saying what the code does, not why or history.
8. **Stay in your lane.** Do not reference or ask for any earlier implementation. If a behavior is unclear, ask me to describe the behavior.
9. **Keep Part A honest.** At the end of each step, list any new decisions as one-line entries I can add to the Decisions log.

## Step plan

- **Step 0: Understand and plan.** Restate the design in your own words in under 200 words. Flag anything in Part A that is ambiguous, contradictory, or risky. Propose the concurrency model, the workspace layout, and the first-draft command, event, and snapshot types as signatures only, with no implementation. Ask me the questions you need answered, using Part C and any others. Then stop.
- **Step 1: Skeleton.** Create the workspace and the types from Step 0. It must compile, with no logic.
- **Step 2: Process hygiene.** Spawn, graceful stop, force kill, and tree kill, with the no-orphan guarantee on both OSes, using a harmless test child. Include the abrupt-kill test from A6. Nothing else is built until this passes on both OSes.
- **Step 3: Recorder.** The `Recorder` trait, the yt-dlp implementation, and the fake recorder. Ask me for real command lines and output first.
- **Step 4: Detector.** The `Detector` trait and the yt-dlp implementation. Ask me for real command lines and output first.
- **Step 5: Engine loop.** Targets, polling, supervised recordings, the snapshot, the event stream, and the delivery rules from A4.
- **Step 6: Default policy and CLI.** The minimal policy and the headless runner.
- **Step 7: Hardening.** Review the code against A6 and A8 line by line, find violations, and fix them. Review shutdown behavior and panic containment.

Begin with **Step 0**.

---

# PART C: Things only you can fill in

Fill these in before the first chat and paste them under Part A. Short, plain, behavior-level notes work best.

## C1. Real yt-dlp commands

[FILL IN]

- The command you use to check whether a channel is live, plus a sample of its real output when the channel is live and when it is offline.
- The command you use to record, with any flags that matter, plus a sample of its real stdout/stderr during a recording and when it ends.
- Which sites or URL shapes the forked yt-dlp is expected to handle in v1.

## C2. Failure stories, written as behavior

[FILL IN] The 3 to 5 worst things that went wrong in the earlier prototype, described as symptoms, not code. Each one becomes an explicit requirement or test. Use this format:

- **What happened:** (for example, "after the app crashed, yt-dlp kept running and recorded for hours")
- **Why it was bad:** (for example, "two recordings of the same stream, disk filled up")
- **What must be true instead:** (for example, "no recorder survives the engine")

Examples of the kind of thing to include: orphaned processes, duplicate recordings of one stream, a recording that was never reported as finished, a hung check that blocked everything else, output that filled a pipe and deadlocked.

---

## Notes to self (not for pasting)

- Free-plan tip: don't paste Part B again after the first chat. Say "Continue from Step N" and paste Part A only.
- When Claude proposes the types in Step 0, read them carefully. The commands, events, and snapshot are the contract that every future layer depends on, and they are the hardest thing to change later.
- If a chat gets long and Claude starts to forget Part A, start a new chat with the updated Part A. A fresh chat with a clean brief is better than a long one.
