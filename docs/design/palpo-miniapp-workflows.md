# Palpo mini-app workflows: contribution, project approval and agents

> **Historical workflow proposal.** [ADR 0011](../adr/0011-hagency-server-engagements.md)
> supersedes this document's contribution, project/agent approval and allocation
> flow. Matrix administrators approve server associations; Hagency coordinators
> approve projects and agents in Rinx. The Inbox/notification concepts remain.

- Date: 2026-10-03
- Status: Proposed; source reviewed, no implementation or live validation.
- Refines [ADR 0010](../adr/0010-palpo-admin-miniapp.md).
- Palpo source: `3f1ad3ba5c80a7206521b3eb4059a4f1f3786700`.
- Hagency Rust source: `e51a0b1385437c7b5e6893e68233b8173a0db018`.
- OctoSense Glance source: `a30984868a121b36fc1c8a3c2e50753c5fe0410e`.
- Rinx baseline and theme branch are recorded in ADR 0010.

## Scope and identity

Keep the Palpo backend and implement the three requested human workflows in
OctoScript pages inside Rinx. Use the existing Matrix login and the session
adapter in ADR 0010. Add the request/decision records missing from today's
backend; retain its registration, project and agent operations underneath.

Distinguish the Palpo homeserver (the project organization/server Hagency connects
to) from an individual project and its Matrix room. Stage 1 connects a Hagency
fleet to that homeserver. Stage 2 grants a project access to selected contributed
resources. Stage 3 allocates agents from those resources. Start with one fleet
per project, matching today's `project.fleetId`; cross-fleet projects need a
separate contract change.

| Role | Responsibility |
| --- | --- |
| Applicant/project representative | Requests the association of a resource provider with the homeserver; may also be the contributor |
| Palpo administrator | Approves/rejects resource association and project requests using existing server-admin authority |
| Hagency owner/operator | Receives the authorized configuration, imports it on their runtime, approves/rejects agent allocations and top-ups |
| Project owner | Requests the project, creates/manages its agents, requests more tokens and handles execution approvals |

One Matrix user may hold several roles. Store requester, fleet owner, project
owner and deciding actor separately. The human Hagency owner receives the setup
notification. A coordinator agent is not required: the current Rust fleet service
owns provisioning and the approval bot. [Fleet service][hagency-fleet]

## What exists and what must be added

| Stage | Verified current backend | Addition for the requested workflow |
| --- | --- | --- |
| 1: resource contribution | Administrator directly authorizes/installs a fleet; its exact owner downloads the JSON; connection uses a real Matrix probe | Member-submitted contribution request, admin decision, owner notification and an Inbox |
| 2: project creation | Owner directly creates/registers a project and encrypted approval room; no separate admin decision | Project request, admin approval/rejection, project-to-resource grant and server-enforced activation gate |
| 3: agent creation | Owner submits a named-agent request; Hagency operator approves in its console; Palpo verifies fulfillment and room admission | Scoped operator approval in the mini app, decision delivery to Hagency and persistent notifications |
| 3: ongoing management | Hagency has usage, allocation increase and retirement operations | User top-up requests, scoped usage projection, owner-authorized release, execution receipts and cleanup status |

This is based on [Palpo's documented flow][palpo-readme] and its actual
[routes][palpo-http], [workflow][palpo-workflow] and [service][palpo-service].
It does not assert that these three approval gates already exist in the web UI
or identify the versions deployed at the user's server. The existing interactive
account-signup approval is a useful implementation precedent, but is a different
workflow. [Account worker][palpo-accounts]

## 1. Contribute resources and connect Hagency

| Step | Screen, action and recipient | Backend effect |
| --- | --- | --- |
| Submit | Resources → Connect/contribute Hagency; applicant selects the target homeserver, names the contribution and identifies its local Matrix owner | Save a contribution request with a stable operation ID; notify current Palpo approvers |
| Review | Admin Inbox → Resource association; show applicant, intended owner, server, purpose and connection mode | Approve/reject the exact request revision; record actor and reason |
| Install | Approved request shows registration progress | Reuse `Service.create/install` to allocate the fleet namespace and install its App Service; preserve existing policy checks and retry identities |
| Deliver | Hagency owner's Inbox: “Approved — download configuration”; applicant also gets the decision, without credential access | After installation succeeds, enable owner-only `credentials` retrieval; persist notification delivery separately |
| Import | Owner downloads JSON using Rinx's save/export UI and loads it on the Hagency server | Reuse Hagency's existing parser/importer; no credentials in the mini-app script, chat event or notification payload |
| Verify | Owner opens the resource page → Verify connection; foreground can resume an already-authorized verification | Reuse `Workflow.connect`: exact reception/probe evidence and current registration generation must verify |
| Complete | Owner/applicant receive “Connected”; admins see a ready contribution | Show observed roles/resources from Hagency's authenticated publication |

Proposed business stages:

`pending_admin → approved → installing → awaiting_import → verifying → connected`

Rejection/cancellation end a pending request. Installation/probe failures retain
the same request and expose Retry. Track approval, installation, credential
delivery and connection as separate facts: downloaded JSON does not prove import,
and a heartbeat does not prove the required Matrix probe succeeded. Do not claim
to observe file import if no authenticated runtime evidence distinguishes it.

The JSON format stays compatible with the existing owner download. Rinx's trusted
host fetches the credential response and passes it to the native save/export
mechanism; the mini app receives only a file-operation result and redacted
metadata. On mobile use a private temporary file and explicit OS export action.
Opening the notification never downloads or shares credentials automatically.
Repeat authorized downloads recover the same current version; a stale card after
revocation cannot retrieve credentials. The requester cannot download another
person's fleet configuration merely because they submitted its association.

Existing Hagency supports both console import and the offline command
`hagency registration --state-dir <state> import --file <download.json> --homeserver <url>`.
The offline command requires the service to be stopped; the console import can
activate transport in a running service. This is the operator's explicit file
installation step requested in the product flow, not a required Palpo browser
visit. One-time automatic pairing is optional future work. [CLI import][hagency-import],
[running import][hagency-live-import]

## 2. Request and activate a project on contributed resources

| Step | Screen, action and recipient | Backend effect |
| --- | --- | --- |
| Submit | Projects → New project; owner chooses a connected fleet, offered resources, project name/purpose and optional existing room | Persist proposed owner, room and resource selection; validate visible resources and current room ownership without activating the project |
| Review | Admin Inbox → Project request | Approve/reject the frozen request and its permitted resources; notify the project owner |
| Prepare | Owner's open request resumes setup after approval; an offline owner receives “Approved — open to finish setup” | Reuse project creation under the owner's Matrix authority, preserving deterministic room/request IDs |
| Activate | Project page shows room setup and approval-channel readiness | Verify project binding, owner membership/power and required Hagency identities; mark active only after the required setup is proven |
| Use | Owner receives “Project ready”; Create agent becomes available | Every agent submission checks the current approved resource grant as well as the existing Matrix membership/room checks |

Proposed stages:

`pending_admin → approved → awaiting_owner_setup → preparing → active`

An already open owner session can continue automatically after approval, so
`awaiting_owner_setup` may be brief. A backend continuation may use an explicitly
delegated, still-valid owner session; if unavailable, retain approval and resume
when the owner returns. Never call today's `createProject(input, admin, adminToken)`
on their behalf: that function assigns the actor as owner and writes Matrix state
as that actor. Do not keep an unbounded user token merely to avoid a resumable step.

Add a durable `ProjectResourceGrant` binding approved request, owner, fleet,
allowed resource IDs/roles and revision. Check it at project activation and each
new agent/top-up request; forward the relevant grant proof/revision to Hagency
for runtime control. All equivalent backend paths must enforce the new gate,
including browser/API callers, or it is only cosmetic. Existing projects need
an explicit audited migration policy; absent approval must not silently grant
new resources. Preserve already-running work unless a separate revoke decision
explicitly targets it.

Initial project approval grants eligibility to use selected resources. Actual
tokens are allocated at Hagency's agent approval. Do not label a project budget
as reserved capacity without a runtime reservation ledger. A hard aggregate
project budget would require atomic checks across all its agents and top-ups;
that is additional backend work, not an effect of an admin approval card.

Retain current room constraints: the project channel is invite-only plaintext;
the owner's execution-approval channel is private Megolm with the owner and
Hagency approval identity. Show missing bot membership or key readiness as setup
work. An approved project cannot become active just because a database row exists.

## 3. Create and manage agents

| Action | Mini-app process | Completion evidence |
| --- | --- | --- |
| Create | Owner opens an active project, names an agent, selects an allowed resource/role and requests initial tokens | Palpo records/sends the existing named-agent request; Hagency verifies and admits it |
| Approve/reject | Hagency owner/operator gets an Inbox item and reviews the project, agent definition and allocation amount | A scoped command invokes Hagency's existing domain decision; current capacity and authority still apply |
| Provision | Owner sees Awaiting Hagency review → Approved → Preparing agent | Runtime fulfillment succeeds and the actual agent is verified in the target Matrix room before Ready is shown |
| Inspect | Agent detail shows status, approved tokens, observed usage/remaining and last observation | New scoped projection from Hagency; unknown or stale values remain explicit |
| Request more tokens | Owner enters an increase and reason on the existing agent | Durable top-up request goes to that fleet's authorized operator; approval invokes the existing allocation-increase command once |
| Stop/resume | Owner uses actions explicitly allowed by the project's management policy | Runtime receipt confirms the change; a local button state is not proof |
| Remove/release | Project owner confirms release of their agent; this need not wait for a second admin approval | Owner-authorized command requests retirement; show Retiring until runtime and Matrix cleanup are confirmed |
| Execution approval | Owner opens the original private request from the agent | Existing trusted execution-approval protocol remains distinct from allocation approval |

Agent creation stages:

`queued → pending_hagency → approved → provisioning → ready`

Top-up stages:

`pending_hagency → approved → applying → applied`

Removal stages:

`retirement_requested → retiring → retired`

Every flow also exposes rejected, cancelled/expired where valid, and retryable
execution failures. A decision recorded by Palpo but not yet accepted by Hagency
is “Waiting for Hagency,” not an applied allocation. Provider capacity failure
does not repeatedly reopen the approval or auto-increase a requested amount.

Hagency already has allocation increase, quota pause/resume and retirement.
Reuse its domain functions and command replay handling. Its current outbound
worker accepts request/probe, so new authenticated control commands are needed
to exercise operator functions from Rinx. Bind the operator's Matrix identity
to the fleet once through authenticated operator enrollment; consent alone or
Palpo admin status cannot confer control of an independently owned runtime.
[Engagement operations][hagency-engagements], [transport][palpo-outbound]

During the runtime's authenticated setup, the operator must explicitly enable
the remote decision capability for this imported fleet and bind its trusted
Palpo issuer and operator identities. Publish support for the versioned command
and project-grant contract before Rinx enables those actions. An older connected
runtime without this support is “Upgrade required for in-app decisions,” not an
apparently working Approve button. Recheck current delegation, project grant,
registration generation and command expiry when executing queued work.

An owner release is a narrow server-authorized action scoped to their engagement,
not access to Hagency's broad operator console. If the fleet is offline, report
retirement pending and retry on reconnection. Server-side access removal may
finish separately; preserve both results. Do not delete history or claim to stop
an uncontactable process. Top-up keeps the same agent and usage history. Current
quota pause gates new turns and lets an in-flight turn finish. [Allocation][hagency-allocation]

## A personal My Actions room, following OctoSense Glance

Give each Matrix account a dedicated private **My Actions** room, shown near
the top of the Rinx room list by default. Use one room for the user's Palpo
actions on this homeserver, with a card per workflow/action step. Creating a
new Matrix room for every approval would fragment the work and is not required.
The person can change the room's position without deleting pending actions.

Inside Rinx this room opens an action board inspired by OctoSense Glance:

- **Needs my action:** pending approvals, configuration handoff, setup and
  recovery tasks; order by deadline/overdue status and then age.
- **Waiting:** requests the person submitted and operations awaiting another
  actor or runtime completion.
- **History:** completed, rejected, cancelled and expired actions.

Show cards with the app/source, project/resource, requester, current status,
age/due time and the available actions. Examples: a project card with Review /
Approve / Reject; an approved contribution with Download configuration / Verify
connection; an agent card with requested tokens and Review allocation.
These actions are rendered by the installed Palpo mini app's trusted card UI,
within its current account-bound permissions. Small decisions can complete in
the card; larger forms expand into a mini-app sheet/detail view inside Rinx.
The room shell handles layout, navigation and card lifecycle. It does not
reimplement all Palpo application pages in Rust.

Clicking a system notification opens My Actions and focuses the exact card. It
can expand the card's mini-app detail directly when the action needs a form.
The notification can clear while that card stays pending. A resolved card moves
to History; the workflow and its audit record remain available. Reminders bring
the same card back to attention rather than inserting duplicate tasks. The room
list's pending-action badge is independent of Matrix's unread-message count.

Admins, project owners and Hagency operators each receive their own room. A
person holding several roles sees their combined permitted actions there. A
request requiring any one of several admins appears in each eligible admin's
room; one accepted decision updates all copies. No shared personal room gives
one user access to another user's projects or owner-only configuration.

### Room binding and synchronization

Persist a server-side binding of account, homeserver, room ID, workflow bot and
purpose. Create/recover the room idempotently; the only initial participants are
that user and the designated workflow bot. Use invite-only membership and bound
service identity checks. Rinx joins through the normal authenticated room flow
under the user's approved service setup. Do not infer trust from a room's name.
Leaving the room stops that room's delivery; the next explicit My Actions setup
can repair/recreate its binding without discarding canonical pending work.

Matrix messages carry stable workflow/action references and a readable fallback.
Rinx obtains current details and allowed actions from Palpo, then groups updates
and reminders under the same card. The ordinary message history remains available
as a fallback for clients without this room view. A lost/redacted Matrix message
does not delete a workflow: the board reloads from the recipient-filtered Inbox.
Never execute Splash source attached by an arbitrary Matrix sender; load the
installed, verified mini-app card template and treat event content as data.

The existing minimal plaintext notification-room policy below applies here:
only non-secret routing metadata and generic notices enter room history. Details
are fetched under the viewer's current authorization. The separate encrypted
Hagency execution-approval room and its bot/device binding remain intact. My
Actions can link to its original approvals through the trusted host; it does not
copy private keys, credentials or approval authority into a new room.

### What to reuse from Glance

The reviewed OctoSense host has `glance.publish`, `glance.withdraw` and
`glance.list`. A stable `(app, card_id)` replaces an existing card, publisher
identity comes from the host, and interactive tiles run under the publisher's
policy. Cards can open their publisher's app/route; notification behavior is
separate from the live card. Reuse those concepts, card/theme components and
bounded execution where compatible. [Glance service][octosense-glance],
[card runtime][octosense-glance-card]

Its current store has card expiry (24 hours by default, at most seven days),
user dismissal and small per-app/display caps. Those are presentation rules,
not suitable as the durable task ledger. Palpo's Inbox remains authoritative;
pagination must expose every pending action. Card expiry, dismissal or a full
Glance panel never resolves a workflow. In My Actions, Remind me later affects
the reminder schedule; a task leaves Needs my action when its human step is
resolved or no longer assigned to that user.

Standalone Rinx currently rejects the `glance` capability in catalog admission
and has no `glance.*` dispatch adapter. Permission-description text alone does
not establish support. This design needs a room/card host integration; it must
not claim the OctoSense API already works in Rinx. If Rinx is hosted in OctoSense,
an optional Glance summary such as “3 Palpo actions pending” can route to this
same room. That summary is a second entry point to the same tasks, not another
Inbox or another approval state machine.

## One Inbox and persistent process state

Use one Palpo mini app with role-aware pages: Inbox, Resources, Projects and
Agents. Its Inbox and the My Actions room are two views of the same records,
using Needs my action, Waiting and History. A person with several roles sees
all their relevant work without logging in again or switching apps.
Each detail page shows who must act next, the decision history, execution status
and a concrete action: Review, Download configuration, Verify connection, Open
project, Request tokens or Retry cleanup.

Palpo owns the canonical workflow records; Hagency owns capacity and runtime
execution facts. Matrix is the notification channel. The OctoScript instance
need not stay open or run a background polling loop for the process to continue.

Proposed durable records in the existing Palpo service:

| Record | Required information |
| --- | --- |
| Workflow | Kind, stable ID, requester, owner, server/fleet/project/engagement references, input digest, revision, decision, execution stage, timestamps and expiry |
| Decision | Actor verified by backend, operation ID, expected workflow revision, decision/amount/reason and result |
| Project resource grant | Approved owner/project/fleet/resource scope, approval reference, current revision and state |
| Runtime command | Stable command ID, exact operation/arguments digest, actor/scope proof, authority revision, fleet generation, expiry and runtime receipt |
| Inbox entry | Recipient, workflow/revision, next action, read timestamp; recipients/actions derived on the server |
| Action-room binding | Account, homeserver, room ID, trusted workflow bot, purpose and binding revision |
| Notification outbox | Notification ID, recipient, workflow revision, channel, delivery attempts, next retry and returned Matrix event ID |
| Reminder schedule | Workflow/action-step revision, recipient, policy, reminder ordinal, next due time, snooze time and cancellation state |

Commit workflow changes, audit and intended notifications atomically using the
existing store's transaction facility. A worker delivers notifications after
commit, with retries/backoff. Notification failure never rolls back an approved
decision or hides its Inbox entry. Do not await network calls inside the SQLite
transaction. Stable Matrix transaction IDs and client notification IDs handle
retries; do not assume the transport is exactly-once. [Current store][palpo-store]

Inbox APIs use bounded cursor pagination. Return current allowed actions and
next actor. Keep read state separate from whether an action is still required.
Two admins deciding concurrently cannot both win: compare the expected revision,
record one decision and make the other client refresh. Reusing an operation ID
with different content conflicts. Rejected requests can be edited/resubmitted
as a new revision requiring a fresh verdict; old cards cannot approve new input.

## Notifications and reopening the right page

1. At submission, notify the next approver. After a decision, notify the applicant
   and the next operator/owner. After setup, notify the people waiting for it.
   Coalesce frequent agent statistics; notify actionable quota hold, ready,
   decision and failure transitions instead of every heartbeat.
2. Send a normal Matrix message from a configured, authenticated Palpo workflow
   bot to a validated private recipient room or restricted admin review room.
   Use a readable fallback body and a versioned custom content object containing
   only notification ID, app identity, workflow ID/revision and a typed view.
   Secrets, full request inputs and private approval-room IDs are not embedded.
3. Use `m.text` with appropriate `m.mentions` for action-required messages;
   ordinary informational updates can be quiet. Respect user notification
   settings. A registered Matrix pusher/push gateway and native platform delivery
   are needed for background device alerts. Matrix sync alone is not a promise
   of notifications while Android/OpenHarmony has suspended Rinx. [Matrix notifications][matrix-notifications]
4. Rinx verifies the bot/room binding and maps the typed notification to the
   registered Palpo package. On tap, open My Actions in the correct account and
   homeserver, focus the workflow card and expand its mini-app detail if needed.
   Reuse host login, fetch its current record and render actions.
   A forged card or external URL cannot select a credential destination, install
   an arbitrary package or execute an approval. Missing package/permission opens
   the normal trusted installation/consent flow and retains the pending route.
5. The detail screen provides Approve/Reject and their fields. It submits through
   the same authenticated decision service, whether opened from chat or Inbox.
   Fetch current state before enabling a decision; old notification payloads are
   navigation hints, not authority. A resolved request opens its final receipt.
6. Refresh Inbox on launch/foreground and after a decision; optional bounded
   long polling supplies foreground changes. Persist the cursor and recover from
   missed, duplicate or out-of-order Matrix delivery. Multi-device read state is
   stored server-side. Account switching never renders the previous user's data.

Reuse the account-signup worker's durable delivery and request-binding lessons
and Rinx's existing approval-card presentation where appropriate. Do not reuse
`palpo.register_account` or agent tool-approval verdicts for these new workflows:
the handlers, authority and consequences differ. A notification's Review action
opens the mini app; it does not execute a command from arbitrary message JSON.

The initial minimal notification rooms may remain private plaintext because
they carry only navigation metadata and generic messages, with details fetched
through authenticated APIs. They must not reuse or weaken the encrypted private
execution-approval rooms. If full workflow details are sent through encrypted
notifications later, add a real SDK-backed encrypted bot rather than assuming
the current Palpo signup worker can encrypt/decrypt them.

## Opening a notification, unfinished actions and reminders

Treat notification visibility, reading and action completion as independent:

| What happened | Notification and workflow result |
| --- | --- |
| User taps a phone/desktop alert | Rinx opens My Actions at the relevant card, expanding its mini-app detail when needed; the OS alert may be dismissed. The Matrix message is not deleted by the tap |
| User reads the card or detail | Mark it seen/read as appropriate; its pending action and Needs my action entry remain |
| User closes the mini app without deciding | Persist any supported draft; leave the action pending and its reminder schedule active |
| Backend accepts a decision | Resolve this human action, cancel its reminders and refresh all relevant clients; runtime work may still be in progress |
| Process reaches another human step | Create/activate the next action for its correct recipient and start that step's notification schedule |

Show a pending-action count on the mini app separately from Matrix's unread
count. Reading can clear an unread indicator without removing Needs my action.
History retains resolved requests. A read receipt or dismissed banner never
means approved, rejected, configuration imported or agent provisioned.

Yes, an unfinished action should be reminded again. This is Palpo workflow
behavior we must implement; Matrix delivers the resulting messages and push
notifications but does not decide whether a business action remains unfinished.
Run the reminder scheduler on the server, independently of any Rinx instance.

For ordinary contribution/project/allocation requests, propose configurable
defaults: notify immediately, then remind at 1 hour, 24 hours and 48 hours after
the action became pending. After three reminders keep the action visible as
overdue; any escalation goes only to explicitly configured, currently authorized
recipients. This cadence is a proposal, not existing behavior. Workflow deadlines
take precedence: expiry stops reminders and follows the defined expiry outcome;
there is no automatic approval/rejection merely because a reminder was ignored.

Respect quiet hours, notification preferences and an explicit Remind me later
control. Snooze postpones reminders, not the request deadline or its pending
status. Opening the screen does not reset the reminder clock. Coalesce overdue
reminders after downtime instead of sending every missed reminder at once.

Before enqueue and again before delivery, check that the same action revision is
still pending, the recipient still needs to act and has permission, and the item
is not expired, cancelled or snoozed. One administrator's accepted decision stops
that action's reminders for all approvers. A notification already in flight may
arrive after completion; opening it fetches the completed result with no active
decision button. Server-side revision checks and idempotent decisions prevent
duplicate approvals, even when two clients submit concurrently.

Each scheduled reminder has a new notification ID, with an optional Matrix reply
relation to the original card, so it can produce a fresh alert. Retries of that
same reminder reuse its delivery identity; they do not create another scheduled
reminder or workflow. Count a reminder once when Matrix accepts delivery, not on
every failed attempt, and never equate that acceptance with the person seeing it.
User/device settings can still suppress the OS alert; Inbox remains authoritative.

Reminder wording follows the actual remaining action. For example, after JSON
export, “Finish importing and verifying your Hagency connection” replaces
“Download configuration.” Once an approval is accepted and Hagency is executing,
show progress/failure updates rather than reminding the administrator to approve
again. Persist reminder policy, due time and uniqueness keys so a worker restart
does not reset counters or send duplicate scheduled reminders.

## Host services and API responsibilities

These are proposed service groups, not existing endpoint names:

| Host service group | Backend responsibility |
| --- | --- |
| `palpo.session.*` | Reuse Rinx login; app-scoped session and existing role checks |
| `palpo.inbox.*`, `palpo.workflows.get` | Recipient-filtered records, read state, cursor and allowed actions |
| `palpo.contributions.submit/decide` | Stage 1 request/decision; reuse fleet installation |
| `palpo.fleets.export_config/verify` | Owner-only host file export and real connection proof |
| `palpo.project_requests.submit/decide/activate` | Stage 2 approval and owner-authenticated project setup |
| `palpo.agents.request/get/release` | Existing request admission, scoped runtime status and owner release |
| `palpo.allocations.request_increase/decide` | Stage 3 top-up intent and existing Hagency allocation command |
| `palpo.agent_requests.decide` | Authenticated fleet-operator decision delivered to Hagency |

The same business handlers serve any retained browser frontend, so introducing
approval gates cannot leave bypasses in old routes. Keep credential export in
trusted native code. Keep provider commands finite and schema-checked; retain
the existing outbound connection topology and Hagency capacity engine.

## Delivery and validation

1. Land login/role integration, workflow records, Inbox/outbox and trusted
   notification routing. Demonstrate member submission and admin decision.
2. Complete stage 1 through real config export/import and Matrix probe. The
   owner operates Hagency; all Palpo interactions happen in Rinx.
3. Complete stage 2 through owner-authenticated setup and enforced resource grant.
4. Complete stage 3 through operator approval, real agent admission, observed
   usage, one top-up and verified retirement.

Frontend parity in ADR 0010 is a foundation milestone. The requested product is
complete only when all three workflows above operate in Rinx; new backend gates
are explicit work, not reasons to redirect people to the web frontend.

Validate with separate admin, contributor/operator and project-owner accounts,
plus an unauthorized member. Cover:

- Approve and reject stage 1; deny non-owner JSON export; repeat an owner download
  without creating another fleet; import the real file and verify a real probe.
- Approve and reject stage 2; prove owner identity stays correct, pre-approval
  agent requests fail and both old/new API routes enforce the resource grant.
- Hagency approval creates one real agent in the selected project; a top-up
  retry applies once; usage distinguishes unknown/stale; release proves runtime
  cleanup and Matrix removal or exposes partial failure honestly.
- Two concurrent approvers, stale revisions, revoked roles, duplicate commands,
  offline runtime, lost responses, worker crash after commit/before notification,
  rejected edits and permission expiry during owner setup.
- Mini app closed when notification arrives, missed notification recovered from
  Inbox, two devices, account switch, deep-link forgery and revoked grants.
- Personal action-room binding and privacy, one card across many reminders,
  inline versus expanded actions, all approver copies resolving after one verdict,
  room leave/recovery and complete pending-task pagination beyond Glance caps.
- Tap/read without deciding, reminders at configured times, snooze/quiet hours,
  no reminder after another approver decides, expiry and in-flight late delivery;
  restart recovery sends one reminder and never repeats an accepted decision.
- Actual Makepad input/draw instrumentation for the script pages, shared themes,
  form drafts and file-export action; Android/OpenHarmony device notification,
  touch, Back, file picker and background/resume evidence before platform claims.

This document changes no live service and is not evidence those tests have run.

## Sources

[palpo-readme]: https://github.com/palpo-im/palpo/blob/3f1ad3ba5c80a7206521b3eb4059a4f1f3786700/web-admin/README.md
[palpo-http]: https://github.com/palpo-im/palpo/blob/3f1ad3ba5c80a7206521b3eb4059a4f1f3786700/web-admin/server.mjs
[palpo-service]: https://github.com/palpo-im/palpo/blob/3f1ad3ba5c80a7206521b3eb4059a4f1f3786700/web-admin/lib/service.mjs
[palpo-workflow]: https://github.com/palpo-im/palpo/blob/3f1ad3ba5c80a7206521b3eb4059a4f1f3786700/web-admin/lib/workflow.mjs
[palpo-accounts]: https://github.com/palpo-im/palpo/blob/3f1ad3ba5c80a7206521b3eb4059a4f1f3786700/web-admin/lib/accounts.mjs
[palpo-store]: https://github.com/palpo-im/palpo/blob/3f1ad3ba5c80a7206521b3eb4059a4f1f3786700/web-admin/lib/store.mjs
[palpo-outbound]: https://github.com/palpo-im/palpo/blob/3f1ad3ba5c80a7206521b3eb4059a4f1f3786700/web-admin/deploy/outbound-v2.md
[hagency-fleet]: https://github.com/hagency-org/hagency-rs/blob/e51a0b1385437c7b5e6893e68233b8173a0db018/knowledge/decisions/adr-187-palpo-fleet-without-coordinator.md
[hagency-import]: https://github.com/hagency-org/hagency-rs/blob/e51a0b1385437c7b5e6893e68233b8173a0db018/native/hagency/src/bootstrap/registration.rs
[hagency-live-import]: https://github.com/hagency-org/hagency-rs/blob/e51a0b1385437c7b5e6893e68233b8173a0db018/native/hagency/src/console/palpo_import.rs
[hagency-engagements]: https://github.com/hagency-org/hagency-rs/blob/e51a0b1385437c7b5e6893e68233b8173a0db018/native/hagency/src/console/engagements.rs
[hagency-allocation]: https://github.com/hagency-org/hagency-rs/blob/e51a0b1385437c7b5e6893e68233b8173a0db018/knowledge/decisions/adr-186-engagement-allocation-pause-and-top-up.md
[matrix-notifications]: https://spec.matrix.org/v1.16/client-server-api/#push-notifications
[octosense-glance]: https://github.com/OctoSense-org/OctoSense/blob/a30984868a121b36fc1c8a3c2e50753c5fe0410e/crates/shell/src/glance.rs
[octosense-glance-card]: https://github.com/OctoSense-org/OctoSense/blob/a30984868a121b36fc1c8a3c2e50753c5fe0410e/crates/shell/src/glance_card.rs
