# Approved circular launcher UI

The main native Win32 window uses the approved 13-node, 14-link graph from
`.build/circle-proposal-20261016`. Node identity is mapped by application ID;
installation/version selection and APPS ordering are unchanged.

The 800x640 and 1280x800 proposal coordinates normalize to the actual client
area. Diameter interpolates from 96 to 126 pixels and conservatively fits both
client axes, including the 784x601 client at the 800x640 minimum outer size.
Buttons have elliptical Win32 regions, circular solid/glass paint, and reject a
captured release outside the circle. Tab uses native dialog navigation; arrow
keys choose the nearest node in that direction. Space/Enter and context menus
retain native button behavior. Focus and hover feedback follow the circle.

Links, header, root/profile context, cluster labels, and footer paint on demand
inside BeginPaint/EndPaint, clipped by child regions. Deferred resize continues
to discard copied pixels and invalidate the parent and all children. There is
no idle repaint timer or animation loop. Aetherglass uses transparent foreground
compositing; other themes use GDI directly.

Regression tests: `approved_anchors_and_app_identity`,
`graph_fits_resize_sweep_without_overlap`, `circular_hits_exclude_corners`, and
`links_do_not_cross_unrelated_nodes`.
The private update-completion fixture writes only under `.build/circle-ui-20261016`.
