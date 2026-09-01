/**
 * Type-surface fixture for `check:guest-types`.
 *
 * TYPE-CHECKED ONLY — never bundled and never executed (`package.json`'s build
 * script names `src/index.tsx`; this file is only in the tsconfig's `include`).
 * That separation is the point: `index.tsx` is a runnable example guest whose
 * output `verify.ts` asserts, so it answers "does the bundle work". This file
 * answers a different question — "does the public type surface of `keel/guest`
 * compile the way its own documentation says it does" — and gets to use shapes
 * purely because they are the ones that used to fail.
 *
 * Every case here is a regression guard for a real gap. A compile error in this
 * file IS the test failing; there is nothing to run.
 */

import { FlatList, SectionList, Text, TextInput, useRemoteRef, View, VirtualizedList } from 'keel/guest';

// ============================================================
// #161 — list components take an element type
// ============================================================

/**
 * The case from #161. `item` must narrow to `string` from `data` alone: no
 * annotation, no cast, no explicit type argument. Before the fix `item` was
 * stuck at `unknown` (TS2322 on `ReactNode`), and neither escape hatch worked —
 * annotating the callback failed on contravariance, and `<FlatList<string>>` gave
 * TS2558 because the declaration was not generic.
 */
export function InferredFromData() {
  return <FlatList data={['a', 'b']} renderItem={({ item }) => <Text>{item.toUpperCase()}</Text>} />;
}

/** An explicit type argument is also accepted, for a call site that wants to pin it. */
export function ExplicitTypeArgument() {
  return (
    <FlatList<string>
      data={['a']}
      renderItem={({ item, index }) => (
        <Text>
          {index}: {item}
        </Text>
      )}
      keyExtractor={(item) => item}
    />
  );
}

/** A non-trivial element type flows into every callback that mentions it. */
interface Row {
  id: number;
  label: string;
}

export function ObjectElements() {
  const rows: Row[] = [{ id: 1, label: 'one' }];
  return (
    <FlatList
      data={rows}
      renderItem={({ item }) => <Text>{item.label}</Text>}
      keyExtractor={(item) => String(item.id)}
      getItemLayout={(data, index) => ({ length: 44, offset: 44 * index, index: data?.length ?? 0 })}
    />
  );
}

/** `SectionList` infers the item type AND the extra fields on a section. */
export function SectionsInferBothTypes() {
  return (
    <SectionList
      sections={[{ title: 'letters', data: ['a', 'b'] }]}
      renderItem={({ item }) => <Text>{item.toUpperCase()}</Text>}
      renderSectionHeader={({ section }) => <Text>{section.title}</Text>}
    />
  );
}

/** `VirtualizedList` infers from `data` through its accessor callbacks too. */
export function VirtualizedInfersFromData() {
  const rows: Row[] = [{ id: 2, label: 'two' }];
  return (
    <VirtualizedList
      data={rows}
      getItem={(data, index) => data[index]}
      getItemCount={(data) => data.length}
      renderItem={({ item }) => <Text>{item.label}</Text>}
    />
  );
}

// ============================================================
// #162 — every component accepts a remote ref
// ============================================================

/**
 * Verbatim the shape in `useRemoteRef`'s own JSDoc, which did not compile before
 * #162: no prop interface declared `ref` and `GuestComponent<P>` is not a
 * forwardRef shape, so this was TS2322 ("Property 'ref' does not exist").
 */
export function RefOnTextInput() {
  const [inputRef, remoteInput] = useRemoteRef<{ focus(): void }>();
  const focus = async () => {
    await remoteInput?.invoke('focus');
  };
  return (
    <View>
      <TextInput ref={inputRef} placeholder="x" onSubmitEditing={focus} />
    </View>
  );
}

/**
 * `ref` is declared on `BaseProps`, so it is accepted on ANY component — which is
 * the public-surface decision #162 recorded, and it matches the runtime (the
 * reconciler assigns a nodeId to every node). A `ref` that only worked on
 * `TextInput` would be the narrower contract this case rules out.
 */
export function RefOnAnyComponent() {
  const [viewRef] = useRemoteRef();
  const [textRef] = useRemoteRef();
  return (
    <View ref={viewRef}>
      <Text ref={textRef}>any node can be referenced</Text>
    </View>
  );
}

/** The two features compose: a ref on a list whose element type is inferred. */
export function RefOnAnInferredList() {
  const [listRef] = useRemoteRef();
  return (
    <FlatList ref={listRef} data={[1, 2]} renderItem={({ item }) => <Text>{item.toFixed(1)}</Text>} />
  );
}

// ============================================================
// The other half of the acceptance: what must STILL be rejected
// ============================================================
//
// Widening a type is only half a fix — a `renderItem` whose `item` is `any` would
// have made every positive case above compile too, while giving back exactly the
// safety the prop interfaces were written for. `@ts-expect-error` is the gate for
// that direction: the build FAILS if the line below stops erroring, so these
// cases assert an error rather than the absence of one.

/** A mismatched element type must not compile just because inference now works. */
export function WrongElementTypeIsStillAnError() {
  return (
    <FlatList
      data={[1, 2]}
      // @ts-expect-error `item` is `number` here, so a string method is a type error
      renderItem={({ item }) => <Text>{item.toUpperCase()}</Text>}
    />
  );
}

/** An explicit type argument contradicted by `data` must not compile either. */
export function ExplicitTypeArgumentIsEnforced() {
  return (
    <FlatList<string>
      // @ts-expect-error numbers are not the `string` element type pinned above
      data={[1, 2]}
      renderItem={({ item }) => <Text>{item}</Text>}
    />
  );
}

/** A section field that no section actually carries is still unknown. */
export function UnknownSectionFieldIsStillAnError() {
  return (
    <SectionList
      sections={[{ title: 'letters', data: ['a'] }]}
      renderItem={({ item }) => <Text>{item}</Text>}
      // @ts-expect-error `subtitle` is not a field of the inferred section type
      renderSectionHeader={({ section }) => <Text>{section.subtitle}</Text>}
    />
  );
}

/** `ref` is a remote-ref callback, not an arbitrary value. */
export function RefPropIsTyped() {
  return (
    <View
      // @ts-expect-error a string is not a RemoteRefCallback
      ref="not-a-callback"
    />
  );
}
