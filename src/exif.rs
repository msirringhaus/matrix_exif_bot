use exif::{Exif, In, Tag, Value};

fn exif_coord_to_decimal(exif: &Exif, tag: Tag) -> Result<f64, exif::Error> {
    if let Some(f) = exif.get_field(tag, In::PRIMARY) {
        match &f.value {
            Value::Rational(v) => {
                if v.len() != 3 {
                    return Err(exif::Error::InvalidFormat("GPSLatitude not 3 parts long"));
                }
                let res = v[0].to_f64() + v[1].to_f64() / 60.0 + v[2].to_f64() / 3600.;
                Ok(res)
            }
            _ => Err(exif::Error::NotFound(
                "Coord is not a rational number, as the spec demands.",
            )),
        }
    } else {
        Err(exif::Error::NotFound("Coord not in exif-data."))
    }
}

fn exif_coordref_to_char(exif: &Exif, tag: Tag) -> String {
    if let Some(f) = exif.get_field(tag, In::PRIMARY) {
        f.display_value().to_string()
    } else {
        // Coord-ref does not exist. Fallback to normal N / E
        if tag == Tag::GPSLatitudeRef {
            String::from("N")
        } else {
            String::from("E")
        }
    }
}

pub(crate) fn extract_location_from_exif(data: &[u8]) -> Result<String, exif::Error> {
    let exif = exif::Reader::new().read_from_container(&mut std::io::Cursor::new(data))?;

    let mut long_dec = exif_coord_to_decimal(&exif, Tag::GPSLongitude)?;
    let mut lat_dec = exif_coord_to_decimal(&exif, Tag::GPSLatitude)?;
    let lat_ref = exif_coordref_to_char(&exif, Tag::GPSLatitudeRef);
    let long_ref = exif_coordref_to_char(&exif, Tag::GPSLongitudeRef);

    if &long_ref == "W" {
        long_dec = -long_dec;
    }
    if &lat_ref == "S" {
        lat_dec = -lat_dec;
    }
    println!("long: {}, {}", long_dec, long_ref);
    println!("lat: {}, {}", lat_dec, lat_ref);
    Ok(format!("geo:{},{}", lat_dec, long_dec))
}
