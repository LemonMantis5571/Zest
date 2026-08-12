# Zest Domain Context

This glossary records the domain terms used by the desktop turn lifecycle.

| Term | Meaning |
| --- | --- |
| Turn | One user submission and all provider and tool rounds until a terminal outcome. |
| Turn lifecycle | The progression from an accepted submission through progress, interruption, cancellation, or completion. |
| Session | A live parent conversation with its selected provider and runtime state. |
| Thread | The durable conversation record containing the user submission, assistant output, and observable progress. |
| Chat event | An observable update describing transcript progress or turn state. |
| Interruption | A turn paused while it requires an approval or an answer to a question. |
| Cancellation | A requested stop that ends active provider work and terminalizes pending turn state. |
| Completion | A turn that reached a terminal successful or failed outcome and whose durable state was finalized. |
| External worker | A separately authenticated process used for explicitly delegated work. |
