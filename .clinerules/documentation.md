# Documentation

## Normative Language

Use IETF Normative Language to highlight requirements.

```
The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in BCP 14 [RFC2119] [RFC8174] when, and only when, they appear in all capitals, as shown here.
```

When stating requirements, avoid compound sentences - use one normative key word per sentence. This ensures each requirement is standalone and doesn't need complex reasoning to understand it.

## Design Document Outline

Each design document should have roughly the same structure:

- State the problem. Give context as to why this problem is important to solve. If possible, document the specific workloads and user stories we're targeting.
- State the scope of the document. It can help to limit each design to focus discussion.
- State the goals, non-goals, and requirements. If possible, these should be prioritized.
- Outline the available options
  - For each option, give a detailed description of the solution. Include mermaid diagrams, when possible, to improve clarity.
  - Tie the solution back each workload and describe how the solution would fit
  - Provide a list of Pros and Cons for the solution. Each should reference back to our stated goals and requirements.
- Make a recommendation. This should be using the reasoning of all of our requirements and weighing the pros/cons to come to the conclusion.
- If needed, document the next steps and action items for the design.

After the document is reviewed, each document SHOULD have a final decision. If a decision was made that differs from the recommendation, it needs to be
documented as to why that was the case. It SHOULD address how the reasoning in the recommendation could be improved to better align the team in the future.
