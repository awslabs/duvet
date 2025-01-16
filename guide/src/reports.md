# Reports

Duvet provides a `report` command to provide insight into requirement coverage for a project. Each report has its own [configuration](./config.md).

## HTML

The `html` report is enabled by default. It's rendered in a browser and makes it easy to explore all of the specifications being annotated and provides statuses for each requirement. Additionally, the specifications are highlighted with links back to the project's source code, which establishes a bidirectional link between source and specification.

<!-- TODO provide an example link to a report, ideally the Duvet spec report -->

## Snapshot

The `snapshot` report provides a mechanism for projects to ensure requirement coverage does not change without explicit approvals. It accomplishes this by writing a simple text file to `.duvet/snapshot.txt` that can be checked against a derived snapshot in the project's CI. If the snapshot stored in the repo doesn't match the derived snapshot, we know there was an unintentional change in requirement coverage and the CI job fails.

```console
$ duvet report --ci 
 EXIT: Some(1) 
   Extracting requirements 
    Extracted requirements from 1 specifications  
     Scanning sources 
      Scanned 1 sources  
      Parsing annotations 
       Parsed 1 annotations  
      Loading specifications 
       Loaded 1 specifications  
      Mapping sections 
       Mapped 1 sections  
     Matching references 
      Matched 1 references  
      Sorting references 
       Sorted 1 references
      Writing .duvet/snapshot.txt 

 Differences detected in .duvet/snapshot.txt: 
  
 @@ -1 +1,3 @@ 
  SPECIFICATION: [Section](my-spec.md) 
 +  SECTION: [Section](#section) 
 +    TEXT[implementation]: here is a spec 
  
   × .duvet/snapshot.txt 
   ╰─▶ Report snapshot does not match with CI mode enabled. 
```

This is what is known as a "snapshot test". Note that in order for this to work, the `snapshot.txt` file needs to be checked in to the source code's version control system, which ensures that it always tracks the state of the code.
