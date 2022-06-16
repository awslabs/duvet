#!/bin/sh

SPEC_ROOT=duvet-specification

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

if [ $REBUILD == "true" ] ; then
  cd $SPEC_ROOT
  ./util/specification_extract.sh
  cd ..
fi

# $SPEC_ROOT/util/report.js \
#   $(find src -name '*.py') \
#   $(find test -name '*.py') \
#   $(find compliance_exceptions -name '*.txt')
