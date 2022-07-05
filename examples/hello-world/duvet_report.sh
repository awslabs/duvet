#!/bin/sh

SPEC_ROOT=hello-world-specification
# As we make the duvet framework more robust this logic around extracting and rebuilding
# will probably change. For now, do something simple to unblock ourselves. 
REBUILD=false

if [ ! -z $1 ] && [ $1 == "rebuild" ] ; then
  echo "Re-extracting spec because it was explicitly requested"
  REBUILD=true
fi

if [ ! -d $SPEC_ROOT/compliance ] ; then 
  echo "Compliance directory missing, extracting spec"
  REBUILD=true
fi

#cd ./hello-world-specification
#./util/specification_extract.sh
#cd ..

$SPEC_ROOT/util/report.js hello_world.py
