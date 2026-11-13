# A record with no network

A record can come from the saved answers of Palette, or from a candidate file
of a version that is not in Palette yet. Neither needs a key.

## The saved answers of Palette

`--raw-dir` saves the API responses and the time of the read. `--from-raw`
builds the same snapshot from them later, with no network and no key. The two
options exclude each other. An optional ref after `--from-raw` fills
`source.ref`. For a directory of a cluster, a ref of the form
`cluster:<uid>/<profile>` also selects that one profile. Strata checks the ref
against nothing else in the directory. The record keeps the time of the read,
not the time of the rebuild, so `verify` reports the version as unchanged since
Palette answered:

```text
{{#include ../../output/snapshot-raw.txt}}
```

`scope.json` keeps the project. A directory with no `scope.json` builds a
snapshot at tenant scope.

`registries.json` keeps the registry listing of Palette, when the record read
it for the name of the registry of each pack. So `--from-raw` gives each pack
the same `registryName`. A directory with no `registries.json` builds a record
with no `registryName`, and the same root. See
[The registry of each pack](profile-version.md#the-registry-of-each-pack).

`notifications.json` keeps the notifications of a cluster. Palette serves the
full values of a pack in each event, and these values can hold a password.
Before the save, strata writes `********` in place of each `packValues`,
`appTierValues`, and `appTierProperties` that is not empty. Strata masks all of
the values, not only the fields that `passwordFields` names. Strata reads none
of these fields, so `--from-raw` builds the same record.

`repave.json` keeps the repave that waits for approval, when one waits. Each
reason of Palette holds the values of the pack on each side of the change, with
the value of each macro in its place. A macro value can be a secret. Before the
save, strata writes `********` in place of each value that is not empty. Strata
reads only the code, the message, the pack name, and the keys, so `--from-raw`
builds the same record.

`cluster.json` keeps the document of the cluster. `--raw-dir` takes no cluster
group, but a program can save a group with `RawCluster::save_dir`. Palette
gives the values of a group with no mask, and they can hold a password. Before
the save, strata writes `********` over each password, with the rule of the
[record](other-objects.md#the-config-that-a-cluster-group-gives). When strata
masks a value, it writes the values again from their canonical form, with no
comments. So the record of the directory is the record of the fetch.

Some values have no canonical form, because a key is a map or a list. An
example is a template with no quotes, `host: {{ .spectro.var.host }}`. Strata
still finds each password in such values. It changes the line of the password,
and the rest of the text stays, with its comments. When it cannot change one
line, for example for a block scalar, it writes the values again from the parsed
YAML.

A parser cannot write a key that is a map. Then `save_dir` gives `serialize`
and writes no file. A snapshot of the group gives the same error, because the
`valuesHash` of its record needs the same mask. The record of such values holds
`valuesParseError`, and `valuesHash` covers the masked text. So the `valuesHash`
of the directory is the `valuesHash` of the fetch.

`rbacs.json` and `namespaces.json` keep the role bindings and the namespaces
of a cluster. A directory that an older version of strata saved has neither.
The `policy` tier of its record then holds `null` for them.

`errors.json` keeps why Palette did not give the notifications, the repave,
the role bindings, or the namespaces of a cluster. It holds each answer of
Palette that is an error. Strata writes the file only when it holds an error.
So `--from-raw` gives the record of the fetch, with the same errors. A
directory that an older version of strata saved has no `errors.json`, so its
record holds no error.

## A version before you publish it

`--from-candidate` builds a snapshot from a candidate file, or from a profile
export of Palette. See [strata candidate](../candidate.md),
[A profile export of Palette](../../reference/candidate-file.md#a-profile-export-of-palette), and
[Review a change before you publish it](../../use-cases/review.md).
