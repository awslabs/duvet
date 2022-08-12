#!/bin/bash

case "${1}" in
  write)
    npx prettier --config .prettierrc.toml --write -- '**/*.md' !./history/*
    ;;
  check)
    npx prettier --config .prettierrc.toml --check -- '**/*.md'
    ;;
  *)
    echo "mode required!"
    echo "${0} [write/check]"
    exit 1
    ;;
esac
