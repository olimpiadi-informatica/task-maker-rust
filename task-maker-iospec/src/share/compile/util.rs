use crate::gen::Gen;
use crate::gen::Inspect;
use crate::share::gen::gen_string;

pub fn unzip_punctuated<T, U>(p: syn::punctuated::Punctuated<T, U>) -> (Vec<T>, Vec<U>) {
    let mut args = Vec::new();
    let mut puncts = Vec::new();
    for p in p.into_pairs() {
        let (a, p) = p.into_tuple();
        args.push(a);
        if let Some(p) = p {
            puncts.push(p)
        }
    }
    (args, puncts)
}

// pub fn quote_hir<T: FormatInto<()>>(ir: T) -> String {
//     let tokens = quote!(#ir);
//     tokens.to_string().unwrap()
// }

pub fn quote_hir<T: Gen<Inspect>>(ir: &T) -> String {
    gen_string(ir, &Inspect)
}
