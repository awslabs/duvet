import { default as React } from 'react';
import { Routes as RouteConfig, Route } from 'react-router';
import { Index } from './Index';
import { Specification } from './Specification';
import { Section } from './Section';
import { useSection, useSpecification } from './Params';
import { PageTitle } from './PageTitle';

export function Routes() {
  return (
    <RouteConfig>
      <Route index element={<RoutedIndex />} />
      <Route path="/spec/:specification" element={<RoutedSpecification />} />
    </RouteConfig>
  );
}

function RoutedIndex() {
  return (
    <>
      <PageTitle value="" />
      <Index />
    </>
  );
}

function RoutedSpecification() {
  const specification = useSpecification();
  // TODO not found page
  if (!specification) return 'specification not found';
  return (
    <>
      <PageTitle value={specification.title} />
      <Specification specification={specification} />;
    </>
  );
}
