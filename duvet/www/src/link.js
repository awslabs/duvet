// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

import { Link as A } from "react-router-dom";
import B from "@material-ui/core/Link";

export function Link(props) {
  if (props.href) return <B {...props} />;

  return <B component={A} {...props} />;
}
