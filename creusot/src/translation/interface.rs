use super::function::closure_generic_decls;
use crate::{
    backend::{logic::spec_axiom, term::lower_pure},
    clone_map::CloneMap,
    ctx::*,
    translation::{function::closure_contract, ty::closure_accessors},
    util::{self, sig_to_why3},
};
use rustc_hir::def_id::DefId;
use rustc_middle::ty::{ClosureKind, TyKind};
use std::borrow::Cow;
use why3::{
    declaration::{Contract, Decl, LetDecl, LetKind, Module, Use},
    Exp, Ident,
};

pub(crate) fn interface_for<'tcx>(
    ctx: &mut TranslationCtx<'tcx>,
    def_id: DefId,
) -> (Module, CloneMap<'tcx>) {
    debug!("interface_for: {def_id:?}");
    let mut names = CloneMap::new(ctx.tcx, def_id, CloneLevel::Stub);
    let mut sig = util::signature_of(ctx, &mut names, def_id);

    sig.contract.variant = Vec::new();

    let mut decls: Vec<_> = closure_generic_decls(ctx.tcx, def_id).collect();

    if ctx.tcx.is_closure(def_id) {
        decls.push(Decl::UseDecl(Use {
            name: format!("{}_Type", &*module_name(ctx.tcx, def_id)).into(),
            as_: None,
            export: true,
        }));
        let acc: Vec<_> = closure_accessors(ctx, def_id)
            .into_iter()
            .map(|(sym, sig, body)| -> Decl {
                let mut sig = sig_to_why3(ctx, &mut names, sig, def_id);
                sig.name = Ident::build(sym.as_str());
                Decl::Let(LetDecl {
                    kind: Some(LetKind::Function),
                    rec: false,
                    ghost: false,
                    sig,
                    body: lower_pure(ctx, &mut names, body),
                })
            })
            .collect();
        let contract = closure_contract(ctx, def_id).to_why(ctx, def_id, &mut names);

        decls.extend(names.to_clones(ctx));
        decls.extend(acc);
        decls.extend(contract);

        if let TyKind::Closure(_, subst) = ctx.tcx.type_of(def_id).kind() {
            if subst.as_closure().kind() == ClosureKind::FnMut {
                sig.contract.ensures.push(
                    Exp::pure_var("unnest".into())
                        .app_to(Exp::Current(box Exp::pure_var("_1'".into())))
                        .app_to(Exp::Final(box Exp::pure_var("_1'".into()))),
                )
            }
        }
    }

    decls.extend(names.to_clones(ctx));

    match util::item_type(ctx.tcx, def_id) {
        ItemType::Predicate => {
            let sig_contract = sig.clone();
            sig.retty = None;
            sig.contract = Contract::new();
            decls.push(Decl::ValDecl(util::item_type(ctx.tcx, def_id).val(sig)));

            let has_axioms = !sig_contract.contract.is_empty();
            if has_axioms {
                decls.push(Decl::Axiom(spec_axiom(&sig_contract)));
            }
        }
        ItemType::Logic => {
            let sig_contract = sig.clone();
            sig.contract = Contract::new();
            decls.push(Decl::ValDecl(util::item_type(ctx.tcx, def_id).val(sig)));

            let has_axioms = !sig_contract.contract.is_empty();
            if has_axioms {
                decls.push(Decl::Axiom(spec_axiom(&sig_contract)));
            }
        }
        _ => {
            if !def_id.is_local() && !ctx.externs.verified(def_id) && sig.contract.is_empty() {
                sig.contract.requires.push(why3::exp::Exp::mk_false());
            }

            decls.push(Decl::ValDecl(util::item_type(ctx.tcx, def_id).val(sig)));
        }
    }

    let name = interface_name(ctx, def_id);

    (Module { name, decls }, names)
}

pub(crate) fn interface_name(ctx: &TranslationCtx, def_id: DefId) -> Ident {
    format!("{}_Interface", Cow::from(&*module_name(ctx.tcx, def_id))).into()
}
