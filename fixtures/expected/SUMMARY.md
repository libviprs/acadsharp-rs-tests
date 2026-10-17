# Reference output summary

Written by `tools/regenerate_reference.py --accept`. Do not edit by hand:
the next accepted regeneration overwrites it, and a hand edit would be a
claim about the artefacts that nothing checks.

Counts are canonical records, not ACadSharp entities. One block reference
expands into the geometry a viewer draws, so these numbers are deliberately
not the entity counts a DWG inspector would show.

| fixture | version | records | arc | circle | document | ellipse | line | point | polygon | polyline | spline | text | view | warning |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| acadsharp-ac1014 | AC1014 | 366 | 3 | 15 | 1 | 1 | 118 | 40 | 15 | 29 | 2 | 69 | 4 | 69 |
| acadsharp-ac1015 | AC1015 | 361 | 3 | 15 | 1 | 1 | 118 | 40 | 15 | 29 | 2 | 69 | 4 | 64 |
| acadsharp-ac1018 | AC1018 | 361 | 3 | 15 | 1 | 1 | 118 | 40 | 15 | 29 | 2 | 69 | 4 | 64 |
| acadsharp-ac1021 | AC1021 | 357 | 3 | 15 | 1 | 1 | 118 | 40 | 15 | 29 | 2 | 69 | 4 | 60 |
| acadsharp-ac1024 | AC1024 | 368 | 3 | 15 | 1 | 1 | 118 | 40 | 15 | 29 | 2 | 69 | 4 | 71 |
| acadsharp-ac1027 | AC1027 | 368 | 3 | 15 | 1 | 1 | 118 | 40 | 15 | 29 | 2 | 69 | 4 | 71 |
| acadsharp-ac1032 | AC1032 | 368 | 3 | 15 | 1 | 1 | 118 | 40 | 15 | 29 | 2 | 69 | 4 | 71 |
| acadsharp-aecobjects | AC1032 | 1682 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 1679 |
| acadsharp-blockalignmentparameter | AC1032 | 10 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 3 | 5 |
| acadsharp-blockbasepointparameter | AC1032 | 10 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 3 | 5 |
| acadsharp-blockflipparameter | AC1032 | 13 | 0 | 2 | 1 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 3 | 5 |
| acadsharp-blocklinearparameter | AC1032 | 11 | 0 | 2 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 3 | 5 |
| acadsharp-blocklookupparameter | AC1032 | 21 | 0 | 0 | 1 | 0 | 8 | 0 | 0 | 4 | 0 | 0 | 3 | 5 |
| acadsharp-blockpointparameter | AC1032 | 19 | 0 | 3 | 1 | 0 | 6 | 0 | 0 | 0 | 0 | 0 | 3 | 6 |
| acadsharp-blockpolarparameter | AC1032 | 17 | 0 | 2 | 1 | 0 | 6 | 0 | 0 | 0 | 0 | 0 | 3 | 5 |
| acadsharp-blockrotationparameter | AC1032 | 25 | 0 | 4 | 1 | 0 | 12 | 0 | 0 | 0 | 0 | 0 | 3 | 5 |
| acadsharp-blockvisibilityparameter | AC1032 | 21 | 0 | 3 | 1 | 0 | 4 | 0 | 0 | 5 | 0 | 0 | 3 | 5 |
| acadsharp-blockxyparameter | AC1032 | 30 | 0 | 21 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 3 | 5 |
| acadsharp-geoloc | AC1032 | 10 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 3 | 6 |
| acadsharp-empty | AC1032 | 9 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 3 | 5 |
| acadsharp-sample-base | AC1032 | 368 | 3 | 15 | 1 | 1 | 118 | 40 | 15 | 29 | 2 | 69 | 4 | 71 |
| acadsharp-export-sample | AC1032 | 331 | 13 | 42 | 1 | 14 | 130 | 23 | 10 | 33 | 2 | 33 | 9 | 21 |
| g11_codepage | AC1018 | 13 | 1 | 1 | 1 | 0 | 2 | 0 | 0 | 0 | 0 | 2 | 2 | 4 |
| g11_shapes | AC1032 | 13 | 1 | 1 | 1 | 0 | 2 | 0 | 0 | 0 | 0 | 1 | 2 | 5 |
| g13_arc | AC1032 | 9 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 5 |
| g13_bad_extents | AC1032 | 9 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 2 | 5 |
| g13_circle | AC1032 | 9 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 5 |
| g13_deep_blocks | AC1032 | 9 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 2 | 5 |
| g13_dimension | AC1032 | 18 | 0 | 0 | 1 | 0 | 3 | 4 | 2 | 0 | 0 | 1 | 2 | 5 |
| g13_dimension_deep | AC1032 | 169 | 0 | 0 | 1 | 0 | 48 | 64 | 32 | 0 | 0 | 16 | 2 | 6 |
| g13_dimension_shallow | AC1032 | 48 | 0 | 0 | 1 | 0 | 12 | 16 | 8 | 0 | 0 | 4 | 2 | 5 |
| g13_ellipse | AC1032 | 9 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 5 |
| g13_empty_view | AC1032 | 8 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 5 |
| g13_face3d | AC1032 | 14 | 0 | 0 | 1 | 0 | 0 | 0 | 6 | 0 | 0 | 0 | 2 | 5 |
| g13_hatch | AC1032 | 22 | 2 | 0 | 1 | 0 | 10 | 0 | 0 | 0 | 1 | 0 | 2 | 6 |
| g13_insert | AC1032 | 10 | 0 | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 2 | 5 |
| g13_leader | AC1032 | 14 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 11 |
| g13_line | AC1032 | 9 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 2 | 5 |
| g13_long_text | AC1032 | 9 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 2 | 5 |
| g13_many_inserts | AC1032 | 30008 | 10000 | 10000 | 1 | 0 | 10000 | 0 | 0 | 0 | 0 | 0 | 2 | 5 |
| g13_mesh | AC1032 | 11 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 8 |
| g13_mesh_bad_faces | AC1032 | 9 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 6 |
| g13_mirrored_bulge | AC1032 | 9 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 2 | 5 |
| g13_mline | AC1032 | 16 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 13 |
| g13_nonuniform | AC1032 | 9 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 5 |
| g13_ocs_mirror | AC1032 | 14 | 2 | 2 | 1 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 5 |
| g13_ocs_plane | AC1032 | 14 | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 2 | 5 |
| g13_ocs_rotated | AC1032 | 11 | 1 | 1 | 1 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 2 | 5 |
| g13_ocs_skew | AC1032 | 11 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 2 | 6 |
| g13_point | AC1032 | 16 | 0 | 0 | 1 | 0 | 0 | 8 | 0 | 0 | 0 | 0 | 2 | 5 |
| g13_polyface_mesh | AC1032 | 12 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 9 |
| g13_polygon_mesh | AC1032 | 12 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 9 |
| g13_polyline | AC1032 | 11 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 2 | 5 |
| g13_ray_xline | AC1032 | 15 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 2 | 11 |
| g13_scale_16x | AC1032 | 2056 | 0 | 1024 | 1 | 0 | 1024 | 0 | 0 | 0 | 0 | 0 | 2 | 5 |
| g13_scale_1x | AC1032 | 136 | 0 | 64 | 1 | 0 | 64 | 0 | 0 | 0 | 0 | 0 | 2 | 5 |
| g13_scale_4x | AC1032 | 520 | 0 | 256 | 1 | 0 | 256 | 0 | 0 | 0 | 0 | 0 | 2 | 5 |
| g13_slot | AC1032 | 9 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 2 | 5 |
| g13_slot_block | AC1032 | 11 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 2 | 5 |
| g13_solid | AC1032 | 13 | 0 | 0 | 1 | 0 | 0 | 0 | 5 | 0 | 0 | 0 | 2 | 5 |
| g13_spline | AC1032 | 9 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 2 | 5 |
| g13_text | AC1032 | 10 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 2 | 5 |
| g13_tolerance | AC1032 | 12 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 9 |
| g13_two_entities | AC1032 | 10 | 0 | 0 | 1 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 5 |
| g13_unsupported | AC1032 | 9 | 0 | 0 | 1 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 2 | 5 |
| g13_wide_polyline | AC1032 | 9 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 2 | 5 |
| g13_wide_spline | AC1032 | 9 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 2 | 5 |
| g13_wipeout | AC1032 | 13 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 10 |
| g13_xref | AC1032 | 8 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 5 |
| g13_xref_long | AC1032 | 8 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 5 |
| regression_bulge | AC1032 | 9 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 2 | 5 |
| regression_insert_transform | AC1032 | 9 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 2 | 5 |
| regression_text_zero_height | AC1032 | 11 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 2 | 7 |
| regression_dimension_fields | AC1032 | 47 | 0 | 0 | 1 | 0 | 16 | 12 | 4 | 0 | 0 | 7 | 2 | 5 |
| regression_hatch_loops | AC1032 | 11 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 2 | 5 |
| regression_unsupported_entity | AC1032 | 11 | 0 | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 2 | 6 |
| regression_large_entity_count | AC1032 | 50008 | 0 | 0 | 1 | 0 | 50000 | 0 | 0 | 0 | 0 | 0 | 2 | 5 |
| parity_arc | AC1032 | 10 | 2 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 5 |
| parity_circle | AC1032 | 10 | 0 | 2 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 5 |
| parity_ellipse | AC1032 | 10 | 0 | 0 | 1 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 5 |
| parity_spline | AC1032 | 10 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 2 | 5 |
| parity_line | AC1032 | 10 | 0 | 0 | 1 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 5 |
| parity_polyline | AC1032 | 12 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 2 | 5 |
| parity_text | AC1032 | 13 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 4 | 2 | 5 |
| parity_insert | AC1032 | 10 | 0 | 0 | 1 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 5 |
| parity_hatch | AC1032 | 11 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 2 | 5 |
| parity_viewport | AC1032 | 15 | 0 | 0 | 1 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 4 | 7 |
