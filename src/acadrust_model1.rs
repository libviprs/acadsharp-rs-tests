//! `model-1` from acadrust: the records past what a viewer draws.
//!
//! Written against `reference/ACadSharp.Reference/Model1/`, which defines the
//! mapping, and against docs/MODEL_1.md, which defines the written form. Nothing
//! of upstream acadrust is copied: the names below are the DXF names and the
//! fields are read straight off `HeaderVariables`, with no clamp and no default
//! the DXF writer would apply on the way out, because a record says what the
//! reader found and not what a writer would make of it.

use std::collections::HashSet;

use acadrust::document::{
    CadDocument, SemanticNodeV1, SemanticPartV1, SemanticReferenceV1, SemanticRelationshipKindV1,
};
use acadrust::objects::ObjectType;
use acadrust::types::{Color, Handle};

use crate::model1::{Entry, Header, Record, Value};

fn color(c: &Color) -> i64 {
    // The schema's rule for a colour held in an int variable: the AutoCAD colour
    // index, ByLayer 256 and ByBlock 0, and -1 for a true colour, which has none.
    c.index().map_or(-1, i64::from)
}

/// Every header variable acadrust holds a field for, as a record. A name that
/// is not here is `EXCLUDED` for acadrust in `parity/MODEL1.toml`.
pub fn header_records(doc: &CadDocument) -> Vec<Record> {
    let h = &doc.header;
    let mut out: Vec<Record> = Vec::new();
    let mut put = |name: &str, value: Value| {
        out.push(Record::Header(Header {
            name: name.to_string(),
            value,
        }));
    };
    let i = |v: i64| Value::Int(v);
    let b = |v: bool| Value::Int(i64::from(v));
    let d = Value::Double;
    let s = |v: &str| Value::Str(v.replace("\r\n", "\n").replace('\r', "\n"));
    let p3 = |v: &acadrust::types::Vector3| Value::Point3([v.x, v.y, v.z]);
    let p2 = |v: &acadrust::types::Vector2| Value::Point2([v.x, v.y]);

    // ---- modes and units ----
    put("$ANGBASE", d(h.angle_base));
    put("$ANGDIR", i(h.angle_direction.into()));
    put("$AUNITS", i(h.angular_unit_format.into()));
    put("$AUPREC", i(h.angular_unit_precision.into()));
    put("$ATTMODE", i(h.attribute_visibility.into()));
    put("$BLIPMODE", b(h.blip_mode));
    put("$CAMERADISPLAY", Value::Bool(h.camera_display));
    put("$CAMERAHEIGHT", d(h.camera_height));
    put("$CHAMFERA", d(h.chamfer_distance_a));
    put("$CHAMFERB", d(h.chamfer_distance_b));
    put("$CHAMFERC", d(h.chamfer_length));
    put("$CHAMFERD", d(h.chamfer_angle));
    put("$DWGCODEPAGE", s(&h.code_page));
    put("$TDCREATE", d(h.create_date_julian));
    put("$CECOLOR", i(color(&h.current_entity_color)));
    put("$CELTSCALE", d(h.current_entity_linetype_scale));
    put("$CELWEIGHT", i(h.current_line_weight.into()));
    put("$CEPSNTYPE", i(h.current_plotstyle_type.into()));
    put("$CLAYER", s(&h.current_layer_name));
    put("$CELTYPE", s(&h.current_linetype_name));
    put("$CMLJUST", i(h.multiline_justification.into()));
    put("$CMLSCALE", d(h.multiline_scale));
    put("$CMLSTYLE", s(&h.multiline_style));
    put("$DISPSILH", b(h.display_silhouette));
    put("$ELEVATION", d(h.elevation));
    put("$ENDCAPS", i(h.end_caps.into()));
    put("$SORTENTS", i(h.sort_entities.into()));
    put("$EXTNAMES", Value::Bool(h.extended_names));
    put("$XCLIPFRAME", i(h.xclip_frame.into()));
    put("$FACETRES", d(h.facet_resolution));
    put("$FILLETRAD", d(h.fillet_radius));
    put("$FILLMODE", b(h.fill_mode));
    put("$FINGERPRINTGUID", s(&h.fingerprint_guid));
    put("$HALOGAP", i(h.halo_gap.into()));
    put("$HANDSEED", Value::Str(format!("{:X}", h.handle_seed)));
    put("$HIDETEXT", i(h.hide_text.into()));
    put("$HYPERLINKBASE", s(&h.hyperlink_base));
    put("$INDEXCTL", i(h.index_control.into()));
    put("$INSUNITS", i(h.insertion_units.into()));
    put("$INTERSECTIONCOLOR", i(h.intersection_color.into()));
    put("$INTERSECTIONDISPLAY", i(h.intersection_display.into()));
    put("$OBSCOLOR", i(h.obscured_color.into()));
    put("$OBSLTYPE", i(h.obscured_linetype.into()));
    put("$JOINSTYLE", i(h.join_style.into()));
    put("$LASTSAVEDBY", s(&h.last_saved_by));
    put("$LATITUDE", d(h.latitude));
    put("$LENSLENGTH", d(h.lens_length));
    put("$LIMCHECK", b(h.limit_check));
    put("$LOFTANG1", d(h.loft_angle1));
    put("$LOFTANG2", d(h.loft_angle2));
    put("$LOFTMAG1", d(h.loft_magnitude1));
    put("$LOFTMAG2", d(h.loft_magnitude2));
    put("$LOFTNORMALS", i(h.loft_normals.into()));
    put("$LOFTPARAM", i(h.loft_param.into()));
    put("$LONGITUDE", d(h.longitude));
    put("$LTSCALE", d(h.linetype_scale));
    put("$LUNITS", i(h.linear_unit_format.into()));
    put("$LUPREC", i(h.linear_unit_precision.into()));
    put("$LWDISPLAY", Value::Bool(h.lineweight_display));
    put("$MAXACTVP", i(h.max_active_viewports.into()));
    put("$MEASUREMENT", i(h.measurement.into()));
    put("$MENU", s(&h.menu_name));
    put("$MIRRTEXT", b(h.mirror_text));
    put("$NORTHDIRECTION", d(h.north_direction));
    put("$OLESTARTUP", Value::Bool(h.ole_startup));
    put("$ORTHOMODE", b(h.ortho_mode));
    put("$OSMODE", i(h.object_snap_mode.into()));
    put("$PDMODE", i(h.point_display_mode.into()));
    put("$PDSIZE", d(h.point_display_size));
    put("$PELEVATION", d(h.paper_elevation));
    put("$PLIMCHECK", b(h.paper_space_limit_check));
    put("$PLINEGEN", b(h.polyline_linetype_generation));
    put("$PLINEWID", d(h.polyline_width));
    put("$PROJECTNAME", s(&h.project_name));
    put("$PROXYGRAPHICS", i(h.proxy_graphics.into()));
    put("$PSLTSCALE", b(h.paper_space_linetype_scaling));
    put("$PSTYLEMODE", Value::Bool(h.plotstyle_mode));
    put("$PSVPSCALE", d(h.viewport_scale_factor));
    put("$QTEXTMODE", b(h.quick_text_mode));
    put("$REGENMODE", b(h.regen_mode));
    put("$REQUIREDVERSIONS", i(h.required_versions));
    put("$SHADEDGE", i(h.shade_edge.into()));
    put("$SHADEDIF", i(h.shade_diffuse.into()));
    put("$SHADOWPLANELOCATION", d(h.shadow_plane_location));
    put("$SHOWHIST", i(h.show_solid_history.into()));
    put("$SKETCHINC", d(h.sketch_increment));
    put("$SKPOLY", i(h.sketch_type.into()));
    put("$SOLIDHIST", b(h.record_solid_history));
    put("$SPLFRAME", b(h.spline_frame));
    put("$SPLINESEGS", i(h.spline_segments.into()));
    put("$SPLINETYPE", i(h.spline_type.into()));
    put("$STEPSIZE", d(h.step_size));
    put("$STEPSPERSEC", d(h.steps_per_second));
    put("$STYLESHEET", s(&h.stylesheet));
    put("$SURFTAB1", i(h.surface_tab1.into()));
    put("$SURFTAB2", i(h.surface_tab2.into()));
    put("$SURFTYPE", i(h.surface_type.into()));
    put("$SURFU", i(h.surface_u_density.into()));
    put("$SURFV", i(h.surface_v_density.into()));
    put("$TDINDWG", d(h.total_editing_time));
    put("$TDUPDATE", d(h.update_date_julian));
    put("$TDUSRTIMER", d(h.user_elapsed_time));
    put("$TEXTSIZE", d(h.text_height));
    put("$TEXTSTYLE", s(&h.current_text_style_name));
    put("$THICKNESS", d(h.thickness));
    put("$TILEMODE", b(h.show_model_space));
    put("$TIMEZONE", i(h.timezone.into()));
    put("$TRACEWID", d(h.trace_width));
    put("$TREEDEPTH", i(h.tree_depth.into()));
    put("$UCSBASE", s(&h.ucs_base));
    put("$UCSNAME", s(&h.model_space_ucs_name));
    put("$USERI1", i(h.user_int1.into()));
    put("$USERI2", i(h.user_int2.into()));
    put("$USERI3", i(h.user_int3.into()));
    put("$USERI4", i(h.user_int4.into()));
    put("$USERI5", i(h.user_int5.into()));
    put("$USERR1", d(h.user_real1));
    put("$USERR2", d(h.user_real2));
    put("$USERR3", d(h.user_real3));
    put("$USERR4", d(h.user_real4));
    put("$USERR5", d(h.user_real5));
    put("$USRTIMER", b(h.user_timer));
    put("$VERSIONGUID", s(&h.version_guid));
    put("$VISRETAIN", b(h.retain_xref_visibility));
    put("$WORLDVIEW", b(h.world_view));
    put("$XEDIT", Value::Bool(h.xedit));

    // ---- extents, limits and the UCS ----
    put("$EXTMAX", p3(&h.model_space_extents_max));
    put("$EXTMIN", p3(&h.model_space_extents_min));
    put("$INSBASE", p3(&h.model_space_insertion_base));
    put("$LIMMAX", p2(&h.model_space_limits_max));
    put("$LIMMIN", p2(&h.model_space_limits_min));
    put("$PEXTMAX", p3(&h.paper_space_extents_max));
    put("$PEXTMIN", p3(&h.paper_space_extents_min));
    put("$PINSBASE", p3(&h.paper_space_insertion_base));
    put("$PLIMMAX", p2(&h.paper_space_limits_max));
    put("$PLIMMIN", p2(&h.paper_space_limits_min));
    put("$PUCSNAME", s(&h.paper_space_ucs_name));
    put("$PUCSORG", p3(&h.paper_space_ucs_origin));
    put("$PUCSXDIR", p3(&h.paper_space_ucs_x_axis));
    put("$PUCSYDIR", p3(&h.paper_space_ucs_y_axis));
    put("$UCSORG", p3(&h.model_space_ucs_origin));
    put("$UCSXDIR", p3(&h.model_space_ucs_x_axis));
    put("$UCSYDIR", p3(&h.model_space_ucs_y_axis));

    // ---- dimension variables ----
    put("$DIMADEC", i(h.dim_angular_decimal_places.into()));
    put("$DIMALT", b(h.dim_alternate_units));
    put("$DIMALTD", i(h.dim_alt_decimal_places.into()));
    put("$DIMALTF", d(h.dim_alt_scale));
    put("$DIMALTRND", d(h.dim_alt_rounding));
    put("$DIMALTTD", i(h.dim_alt_tolerance_decimal_places.into()));
    put("$DIMALTTZ", i(h.dim_alt_tolerance_zero_tight.into()));
    put("$DIMALTU", i(h.dim_alt_units_format.into()));
    put("$DIMALTZ", i(h.dim_alt_tolerance_zero_suppression.into()));
    put("$DIMAPOST", s(&h.dim_alt_post));
    put("$DIMASO", b(h.associate_dimensions));
    put("$DIMASSOC", i(h.dimension_associativity.into()));
    put("$DIMASZ", d(h.dim_arrow_size));
    put("$DIMATFIT", i(h.dim_fit.into()));
    put("$DIMAUNIT", i(h.dim_angular_units.into()));
    put("$DIMBLK", s(&h.dim_arrow_block));
    put("$DIMBLK1", s(&h.dim_arrow_block1));
    put("$DIMBLK2", s(&h.dim_arrow_block2));
    put("$DIMCEN", d(h.dim_center_mark));
    put("$DIMCLRD", i(color(&h.dim_line_color)));
    put("$DIMCLRE", i(color(&h.dim_ext_line_color)));
    put("$DIMCLRT", i(color(&h.dim_text_color)));
    put("$DIMDEC", i(h.dim_decimal_places.into()));
    put("$DIMDLE", d(h.dim_line_extension));
    put("$DIMDLI", d(h.dim_line_increment));
    put("$DIMDSEP", i(i64::from(u32::from(h.dim_decimal_separator))));
    put("$DIMEXE", d(h.dim_ext_line_extension));
    put("$DIMEXO", d(h.dim_ext_line_offset));
    put("$DIMFRAC", i(h.dim_fraction_format.into()));
    put("$DIMGAP", d(h.dim_line_gap));
    put("$DIMJUST", i(h.dim_horizontal_justification.into()));
    put("$DIMLDRBLK", s(&h.dim_leader_arrow_block));
    put("$DIMLFAC", d(h.dim_linear_scale));
    put("$DIMLIM", b(h.dim_limits));
    put("$DIMLUNIT", i(h.dim_linear_unit_format.into()));
    put("$DIMLWD", i(h.dim_line_weight.into()));
    put("$DIMLWE", i(h.dim_ext_line_weight.into()));
    put("$DIMPOST", s(&h.dim_post));
    put("$DIMRND", d(h.dim_rounding));
    put("$DIMSAH", b(h.dim_separate_arrows));
    put("$DIMSCALE", d(h.dim_scale));
    put("$DIMSD1", b(h.dim_suppress_line1));
    put("$DIMSD2", b(h.dim_suppress_line2));
    put("$DIMSE1", b(h.dim_suppress_ext1));
    put("$DIMSE2", b(h.dim_suppress_ext2));
    put("$DIMSHO", b(h.update_dimensions_while_dragging));
    put("$DIMSOXD", b(h.dim_suppress_outside_ext));
    put("$DIMSTYLE", s(&h.current_dimstyle_name));
    put("$DIMTAD", i(h.dim_text_above.into()));
    put("$DIMTDEC", i(h.dim_tolerance_decimal_places.into()));
    put("$DIMTFAC", d(h.dim_tolerance_scale));
    put("$DIMTIH", b(h.dim_text_inside_horizontal));
    put("$DIMTIX", b(h.dim_force_text_inside));
    put("$DIMTM", d(h.dim_tolerance_minus));
    put("$DIMTMOVE", i(h.dim_text_movement.into()));
    put("$DIMTOFL", b(h.dim_force_line_inside));
    put("$DIMTOH", b(h.dim_text_outside_horizontal));
    put("$DIMTOL", b(h.dim_tolerance));
    put("$DIMTOLJ", i(h.dim_tolerance_justification.into()));
    put("$DIMTP", d(h.dim_tolerance_plus));
    put("$DIMTSZ", d(h.dim_tick_size));
    put("$DIMTVP", d(h.dim_text_vertical_pos));
    put("$DIMTXT", d(h.dim_text_height));
    put("$DIMTZIN", i(h.dim_tolerance_zero_suppression.into()));
    put("$DIMUPT", b(h.dim_user_positioned_text));
    put("$DIMZIN", i(h.dim_zero_suppression.into()));
    out
}

/// One `layer` record per LAYER entry (docs/MODEL_1.md): the fields by the DXF
/// code they come from, with the colour as an index or -1 for a true colour.
pub fn layer_records(doc: &CadDocument) -> Vec<Record> {
    doc.layers
        .iter()
        .map(|l| {
            let (color, true_color) = match l.color {
                Color::Rgb { r, g, b } => (
                    -1,
                    (i64::from(r) << 16) | (i64::from(g) << 8) | i64::from(b),
                ),
                other => (other.index().map_or(-1, i64::from), -1),
            };
            let flags = i64::from(l.flags.frozen)
                | (i64::from(l.flags.frozen_in_new_viewport) << 1)
                | (i64::from(l.flags.locked) << 2);
            Record::Entry(Entry::new(
                "layer",
                l.name.clone(),
                vec![
                    Value::Int(flags),
                    Value::Int(color),
                    Value::Int(true_color),
                    Value::Bool(!l.flags.off),
                    Value::Str(l.line_type.clone()),
                    Value::Int(i64::from(l.line_weight.value())),
                    Value::Bool(l.is_plottable),
                ],
            ))
        })
        .collect()
}

/// One `linetype` record per LTYPE entry (docs/MODEL_1.md). The flags of a
/// segment are rebuilt from what acadrust holds as typed data: 1 for an absolute
/// rotation, 2 for embedded text, 4 for an embedded shape. The style is resolved
/// from its handle through the text style table.
pub fn linetype_records(doc: &CadDocument) -> Vec<Record> {
    use acadrust::tables::LineTypeComplexContent;
    doc.line_types
        .iter()
        .map(|lt| {
            let segments = lt
                .elements
                .iter()
                .map(|e| match &e.complex {
                    None => vec![
                        Value::Double(e.length),
                        Value::Int(0),
                        Value::Int(0),
                        Value::Double(1.0),
                        Value::Double(0.0),
                        Value::Point2([0.0, 0.0]),
                        Value::Str(String::new()),
                        Value::Str(String::new()),
                    ],
                    Some(c) => {
                        let (kind, shape, text) = match &c.content {
                            LineTypeComplexContent::Shape { shape_number } => {
                                (4, i64::from(*shape_number), String::new())
                            }
                            LineTypeComplexContent::Text { text } => (2, 0, text.clone()),
                        };
                        let style = doc
                            .text_styles
                            .iter()
                            .find(|t| t.handle == c.style_handle)
                            .map(|t| t.name.clone())
                            .unwrap_or_default();
                        vec![
                            Value::Double(e.length),
                            Value::Int(kind | i64::from(c.absolute_rotation)),
                            Value::Int(shape),
                            Value::Double(c.scale),
                            Value::Double(c.rotation),
                            Value::Point2(c.offset),
                            Value::Str(text),
                            Value::Str(style),
                        ]
                    }
                })
                .collect();
            Record::Entry(Entry::new(
                "linetype",
                lt.name.clone(),
                vec![
                    Value::Str(lt.description.clone()),
                    Value::Int(i64::from(u32::from(lt.alignment))),
                    Value::Double(lt.pattern_length),
                    Value::List(segments),
                ],
            ))
        })
        .collect()
}

/// One `textstyle` record per STYLE entry (docs/MODEL_1.md). The stored last
/// height is the field, not `effective_last_height()`.
pub fn textstyle_records(doc: &CadDocument) -> Vec<Record> {
    doc.text_styles
        .iter()
        // An entry with no name cannot be keyed: it has no record.
        .filter(|t| !t.name.is_empty())
        .map(|t| {
            Record::Entry(Entry::new(
                "textstyle",
                t.name.clone(),
                vec![
                    Value::Str(t.font_file.clone()),
                    Value::Str(t.big_font_file.clone()),
                    Value::Double(t.height),
                    Value::Double(t.width_factor),
                    Value::Double(t.last_height),
                    Value::Double(t.oblique_angle),
                    Value::Bool(t.flags.backward),
                    Value::Bool(t.flags.upside_down),
                    Value::Bool(t.is_shape_file),
                    Value::Bool(t.is_vertical),
                    Value::Bool(t.xref_dependent),
                    Value::Bool(t.annotative),
                ],
            ))
        })
        .collect()
}

/// One `dimstyle` record per DIMSTYLE entry (docs/MODEL_1.md): the `$DIM*`
/// fields, in the order the document gives them. The handle-valued ones are
/// resolved to names through the block record and linetype tables, and a colour
/// is its index, or -1 when acadrust holds a true colour beside it.
pub fn dimstyle_records(doc: &CadDocument) -> Vec<Record> {
    let block_name = |h: acadrust::types::Handle| {
        doc.block_records
            .iter()
            .find(|b| b.handle == h)
            .map(|b| b.name.clone())
            .unwrap_or_default()
    };
    let linetype_name = |h: acadrust::types::Handle| {
        doc.line_types
            .iter()
            .find(|l| l.handle == h)
            .map(|l| l.name.clone())
            .unwrap_or_default()
    };
    doc.dim_styles
        .iter()
        .filter(|t| !t.name.is_empty())
        .map(|t| {
            let mut values = vec![
                Value::Str(t.dimpost.clone()),
                Value::Str(t.dimapost.clone()),
                Value::Double(t.dimscale),
                Value::Double(t.dimasz),
                Value::Double(t.dimexo),
                Value::Double(t.dimdli),
                Value::Double(t.dimexe),
                Value::Double(t.dimrnd),
                Value::Double(t.dimdle),
                Value::Double(t.dimtp),
                Value::Double(t.dimtm),
                Value::Double(t.dimfxl),
                Value::Double(t.dimjogang),
                Value::Int(i64::from(t.dimtfill)),
                Value::Int(i64::from(t.dimtol)),
                Value::Int(i64::from(t.dimlim)),
                Value::Int(i64::from(t.dimtih)),
                Value::Int(i64::from(t.dimtoh)),
                Value::Int(i64::from(t.dimse1)),
                Value::Int(i64::from(t.dimse2)),
                Value::Int(i64::from(t.dimtad)),
                Value::Int(i64::from(t.dimzin)),
                Value::Int(i64::from(t.dimazin)),
                Value::Int(i64::from(t.dimarcsym)),
                Value::Double(t.dimtxt),
                Value::Double(t.dimcen),
                Value::Double(t.dimtsz),
                Value::Double(t.dimaltf),
                Value::Double(t.dimlfac),
                Value::Double(t.dimtvp),
                Value::Double(t.dimtfac),
                Value::Double(t.dimgap),
                Value::Double(t.dimaltrnd),
                Value::Int(i64::from(t.dimalt)),
                Value::Int(i64::from(t.dimaltd)),
                Value::Int(i64::from(t.dimtofl)),
                Value::Int(i64::from(t.dimsah)),
                Value::Int(i64::from(t.dimtix)),
                Value::Int(i64::from(t.dimsoxd)),
                Value::Int(if t.dimclrd_true_color.is_some() {
                    -1
                } else {
                    i64::from(t.dimclrd)
                }),
                Value::Int(if t.dimclre_true_color.is_some() {
                    -1
                } else {
                    i64::from(t.dimclre)
                }),
                Value::Int(if t.dimclrt_true_color.is_some() {
                    -1
                } else {
                    i64::from(t.dimclrt)
                }),
                Value::Int(i64::from(t.dimadec)),
                Value::Int(i64::from(t.dimunit)),
                Value::Int(i64::from(t.dimdec)),
                Value::Int(i64::from(t.dimtdec)),
                Value::Int(i64::from(t.dimaltu)),
                Value::Int(i64::from(t.dimalttd)),
                Value::Int(i64::from(t.dimaunit)),
                Value::Int(i64::from(t.dimfrac)),
                Value::Int(i64::from(t.dimlunit)),
                Value::Int(i64::from(t.dimdsep)),
                Value::Int(i64::from(t.dimtmove)),
                Value::Int(i64::from(t.dimjust)),
                Value::Int(i64::from(t.dimsd1)),
                Value::Int(i64::from(t.dimsd2)),
                Value::Int(i64::from(t.dimtolj)),
                Value::Int(i64::from(t.dimtzin)),
                Value::Int(i64::from(t.dimaltz)),
                Value::Int(i64::from(t.dimalttz)),
                Value::Int(i64::from(t.dimfit)),
                Value::Int(i64::from(t.dimupt)),
                Value::Int(i64::from(t.dimatfit)),
                Value::Int(i64::from(t.dimfxlon)),
                Value::Int(i64::from(t.dimtxtdirection)),
                Value::Str(t.dimtxsty.clone()),
                Value::Str(block_name(t.dimldrblk)),
                Value::Str(t.dimblk_name.clone()),
                Value::Str(t.dimblk1_name.clone()),
                Value::Str(t.dimblk2_name.clone()),
                Value::Str(linetype_name(t.dimltex_handle)),
                Value::Str(linetype_name(t.dimltex1_handle)),
                Value::Str(linetype_name(t.dimltex2_handle)),
                Value::Int(i64::from(t.dimlwd)),
                Value::Int(i64::from(t.dimlwe)),
            ];
            values.push(Value::Bool(t.annotative));
            Record::Entry(Entry::new("dimstyle", t.name.clone(), values))
        })
        .collect()

/// The DXF object name of an object in a dictionary: what ACadSharp calls
/// `ObjectName` and ezdxf `dxftype()`. An object acadrust does not model is
/// `Unknown { type_name }` and keeps the name it was read under.
pub fn object_class(object: &ObjectType) -> String {
    match object {
        ObjectType::Dictionary(_) => "DICTIONARY".into(),
        ObjectType::Layout(_) => "LAYOUT".into(),
        ObjectType::XRecord(_) => "XRECORD".into(),
        ObjectType::Group(_) => "GROUP".into(),
        ObjectType::MLineStyle(_) => "MLINESTYLE".into(),
        ObjectType::ImageDefinition(_) => "IMAGEDEF".into(),
        ObjectType::UnderlayDefinition(d) => d.entity_name().into(),
        ObjectType::PlotSettings(_) => "PLOTSETTINGS".into(),
        ObjectType::MultiLeaderStyle(_) => "MLEADERSTYLE".into(),
        ObjectType::TableStyle(_) => "TABLESTYLE".into(),
        ObjectType::TableContent(_) => "TABLECONTENT".into(),
        ObjectType::Scale(_) => "SCALE".into(),
        ObjectType::ObjectContextData(d) => d.class_name().into(),
        ObjectType::SortEntitiesTable(_) => "SORTENTSTABLE".into(),
        ObjectType::DictionaryVariable(_) => "DICTIONARYVAR".into(),
        ObjectType::VisualStyle(_) => "VISUALSTYLE".into(),
        ObjectType::Material(_) => "MATERIAL".into(),
        ObjectType::ImageDefinitionReactor(_) => "IMAGEDEF_REACTOR".into(),
        ObjectType::GeoData(_) => "GEODATA".into(),
        ObjectType::SpatialFilter(_) => "SPATIAL_FILTER".into(),
        ObjectType::RasterVariables(_) => "RASTERVARIABLES".into(),
        ObjectType::BookColor(_) => "DBCOLOR".into(),
        ObjectType::PlaceHolder(_) => "ACDBPLACEHOLDER".into(),
        ObjectType::DictionaryWithDefault(_) => "ACDBDICTIONARYWDFLT".into(),
        ObjectType::WipeoutVariables(_) => "WIPEOUTVARIABLES".into(),
        ObjectType::BlockVisibilityParameter(_) => "BLOCKVISIBILITYPARAMETER".into(),
        ObjectType::DynamicBlock(d) => d.dxf_name.clone(),
        ObjectType::Associative(d) => d.dxf_name.clone(),
        ObjectType::ClassObject(d) => d.dxf_name().into(),
        ObjectType::DataObject(d) => d.dxf_name().into(),
        ObjectType::Field(_) => "FIELD".into(),
        ObjectType::FieldList(_) => "FIELDLIST".into(),
        ObjectType::RegisteredClass(d) => d.dxf_name.clone(),
        ObjectType::DgnLineStyle(d) => d.dxf_name().into(),
        ObjectType::ProxyObject(_) => "ACAD_PROXY_OBJECT".into(),
        ObjectType::Unknown { type_name, .. } => type_name.clone(),
    }
}

/// Where one dictionary starts, and the path it is keyed under.
fn walk_dictionary(
    doc: &CadDocument,
    handle: Handle,
    path: &str,
    out: &mut Vec<Record>,
    seen: &mut HashSet<Handle>,
) {
    let (entries, hard_owner, cloning) = match doc.objects.get(&handle) {
        Some(ObjectType::Dictionary(d)) => (&d.entries, d.hard_owner, d.duplicate_cloning),
        Some(ObjectType::DictionaryWithDefault(d)) => {
            (&d.entries, d.hard_owner, d.duplicate_cloning)
        }
        _ => return,
    };
    if !seen.insert(handle) {
        return;
    }
    // An entry whose handle names no object is a dangling reference: a reader
    // that resolves the entry to an object has nothing to list.
    let mut listed: Vec<(&str, Handle, String)> = entries
        .iter()
        .filter_map(|(key, h)| {
            doc.objects
                .get(h)
                .map(|o| (key.as_str(), *h, object_class(o)))
        })
        .collect();
    listed.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    out.push(Record::Entry(Entry::new(
        "dictionary",
        path,
        vec![
            Value::Bool(hard_owner),
            Value::Int(i64::from(cloning)),
            Value::List(
                listed
                    .iter()
                    .map(|(k, _, c)| vec![Value::Str((*k).to_string()), Value::Str(c.clone())])
                    .collect(),
            ),
            Value::Int(listed.len() as i64),
        ],
    )));
    for (key, h, _) in &listed {
        let child = if path.is_empty() {
            (*key).to_string()
        } else {
            format!("{path}/{key}")
        };
        walk_dictionary(doc, *h, &child, out, seen);
    }
}

/// The owner a table record or a layout is named by in a path
/// (`table/<kind>/<name>`, `layout/<name>`), when the node is one that has such
/// a name.
fn owner_path(node: &SemanticNodeV1<'_>) -> Option<String> {
    use acadrust::document::SemanticTableRecordV1 as T;
    Some(match node {
        SemanticNodeV1::TableRecord(t) => {
            let (kind, name) = match t {
                T::Layer(r) => ("layer", &r.name),
                T::LineType(r) => ("linetype", &r.name),
                T::TextStyle(r) => ("textstyle", &r.name),
                T::DimStyle(r) => ("dimstyle", &r.name),
                T::BlockRecord(r) => ("block_record", &r.name),
                T::AppId(r) => ("appid", &r.name),
                T::View(r) => ("view", &r.name),
                T::VPort(r) => ("vport", &r.name),
                T::Ucs(r) => ("ucs", &r.name),
                T::Vx(_) => return None,
            };
            format!("table/{kind}/{name}")
        }
        SemanticNodeV1::Object(ObjectType::Layout(l)) => format!("layout/{}", l.name),
        _ => return None,
    })
}

/// One `dictionary` record per DICTIONARY (and DICTIONARYWDFLT) reachable from
/// the root, keyed by the path of entry keys (docs/MODEL_1.md), and per
/// extension dictionary of a table entry or a layout. Those of entities are
/// keyed by handle only and are not written.
pub fn dictionary_records(doc: &CadDocument) -> Vec<Record> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    walk_dictionary(
        doc,
        doc.header.named_objects_dict_handle,
        "",
        &mut out,
        &mut seen,
    );
    let mut owned: Vec<(String, Handle)> = Vec::new();
    doc.semantic_inventory_v1().visit(|part| {
        if let SemanticPartV1::Relationship {
            kind: SemanticRelationshipKindV1::ExtensionDictionary,
            source: SemanticReferenceV1::Resolved(node),
            target: SemanticReferenceV1::Resolved(SemanticNodeV1::Object(target)),
        } = part
            && let Some(path) = owner_path(&node)
        {
            let handle = match target {
                ObjectType::Dictionary(d) => d.handle,
                ObjectType::DictionaryWithDefault(d) => d.handle,
                _ => return,
            };
            owned.push((path, handle));
        }
    });
    for (path, handle) in owned {
        walk_dictionary(doc, handle, &path, &mut out, &mut seen);
    }
    out
}

/// What `emit-acadrust` writes beside the schema 1 stream.
pub fn model1_records(doc: &CadDocument) -> Vec<Record> {
    let mut all = header_records(doc);
    all.extend(layer_records(doc));
    all.extend(linetype_records(doc));
    all.extend(textstyle_records(doc));
    all.extend(dimstyle_records(doc));
    all.extend(dictionary_records(doc));
    all
}

/// Read a DWG or a DXF and render its model-1 stream.
pub fn render(bytes: &[u8], dxf: bool) -> Result<String, String> {
    let doc = if dxf {
        acadrust::io::dxf::DxfReader::from_reader(std::io::Cursor::new(bytes.to_vec()))
            .and_then(|r| r.read())
            .map_err(|e| format!("acadrust could not read this DXF: {e}"))?
    } else {
        acadrust::io::dwg::DwgReader::from_stream(std::io::Cursor::new(bytes.to_vec()))
            .read()
            .map_err(|e| format!("acadrust could not read this file: {e}"))?
    };
    crate::model1::render_stream(&model1_records(&doc))
}
