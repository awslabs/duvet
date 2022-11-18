import { Requirements, Stats } from "./spec"

export function List({spec}) {
  return (
    <>
      <h2>Requirements across specifications</h2>

      <h3>Stats</h3>
      <Stats spec={spec} />

      <h3>Requirements</h3>
      <Requirements
        key={spec.id}
        requirements={spec.requirements}
        showSection={true}
        showSpecification={true}
      />
    </>
  );
}

