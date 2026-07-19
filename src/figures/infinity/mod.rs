pub mod catalog;
pub mod image;

pub use catalog::{
    find_infinity_catalog_entry, infinity_catalog_entry, FigureCatalogEntry, INFINITY_CATALOG,
};
pub use image::{
    decrypt_infinity_figure_data, infinity_figure_number, initialize_infinity_entity_image,
};
