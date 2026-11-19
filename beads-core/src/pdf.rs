//! A one-page PDF "design sheet": title, the design picture and the material
//! list, ready to print. Written by hand (PDF is plain structured text), so no
//! extra libraries are needed. The picture is embedded as a JPEG, which PDF
//! readers decode themselves.

/// US Letter in PDF points (1/72 inch).
const PAGE_W: f64 = 612.0;
const PAGE_H: f64 = 792.0;
const MARGIN: f64 = 40.0;

/// PDF's built-in fonts only cover Western characters: replace anything else.
fn pdf_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let c = match c {
            '×' => 'x',
            '–' | '—' | '−' => '-',
            '‘' | '’' => '\'',
            '“' | '”' => '"',
            '°' => 'o',
            c if c.is_ascii() && !c.is_ascii_control() => c,
            _ => '?',
        };
        if matches!(c, '(' | ')' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Approximate Helvetica width in points (good enough to fit text into columns).
fn text_width(s: &str, size: f64) -> f64 {
    s.chars().map(|c| if c.is_ascii_uppercase() || c == 'm' || c == 'w' { 0.65 } else { 0.5 }).sum::<f64>() * size
}

/// Builds the PDF. `jpeg` is the design picture (`img_w` x `img_h` pixels);
/// `subtitle` is a short line under the title; `lines` is the material list.
pub fn design_sheet(title: &str, subtitle: &str, jpeg: &[u8], img_w: u32, img_h: u32, lines: &[String]) -> Vec<u8> {
    let mut content = String::new();
    let mut y = PAGE_H - MARGIN - 20.0;
    content += &format!("BT /F2 20 Tf {MARGIN} {y:.1} Td ({}) Tj ET\n", pdf_text(title));
    y -= 18.0;
    content += &format!("BT /F1 10 Tf 0.35 g {MARGIN} {y:.1} Td ({}) Tj ET\n0 g\n", pdf_text(subtitle));
    y -= 12.0;

    // The picture: as large as fits in the top part of the page, centred.
    let max_w = PAGE_W - 2.0 * MARGIN;
    let two_columns = lines.len() > 12;
    let list_rows = if two_columns { lines.len().div_ceil(2) } else { lines.len() }.max(1) as f64;
    // Room for the rule, the "Material List" heading and the rows below the picture.
    let list_h = (list_rows * 14.0 + 60.0).min(320.0);
    let max_h = (y - MARGIN - list_h).max(150.0);
    let scale = (max_w / img_w.max(1) as f64).min(max_h / img_h.max(1) as f64);
    let (w, h) = (img_w as f64 * scale, img_h as f64 * scale);
    let x = (PAGE_W - w) / 2.0;
    y -= h;
    content += &format!("q {w:.2} 0 0 {h:.2} {x:.2} {y:.2} cm /Im1 Do Q\n");

    // A thin line, then the material list.
    y -= 16.0;
    content += &format!("0.6 G 0.5 w {MARGIN} {y:.1} m {:.1} {y:.1} l S 0 G\n", PAGE_W - MARGIN);
    y -= 18.0;
    content += &format!("BT /F2 12 Tf {MARGIN} {y:.1} Td (Material List) Tj ET\n");
    y -= 16.0;
    let col_w = (PAGE_W - 2.0 * MARGIN) / 2.0;
    let per_col = if two_columns { lines.len().div_ceil(2) } else { lines.len() };
    for (i, line) in lines.iter().enumerate() {
        let (col, row) = (i / per_col.max(1), i % per_col.max(1));
        let lx = MARGIN + col as f64 * col_w;
        let ly = y - row as f64 * 14.0;
        if ly < MARGIN {
            continue; // past the bottom of the page
        }
        let mut size = 10.0;
        while text_width(line, size) > col_w - 10.0 && size > 6.0 {
            size -= 0.5;
        }
        content += &format!("BT /F1 {size:.1} Tf {lx:.1} {ly:.1} Td ({}) Tj ET\n", pdf_text(line));
    }
    content += &format!(
        "BT /F1 8 Tf 0.5 g {MARGIN} {:.1} Td (Made with Bunch-a-Beads Pattern Designer) Tj ET\n",
        MARGIN - 18.0
    );

    // Assemble the objects and the cross-reference table.
    let mut pdf: Vec<u8> = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::new();
    let mut obj = |pdf: &mut Vec<u8>, body: &[u8]| {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", offsets.len()).as_bytes());
        pdf.extend_from_slice(body);
        pdf.extend_from_slice(b"\nendobj\n");
    };
    obj(&mut pdf, b"<< /Type /Catalog /Pages 2 0 R >>");
    obj(&mut pdf, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    obj(
        &mut pdf,
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_W} {PAGE_H}] /Contents 4 0 R \
             /Resources << /Font << /F1 5 0 R /F2 6 0 R >> /XObject << /Im1 7 0 R >> >> >>"
        )
        .as_bytes(),
    );
    let mut stream = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
    stream.extend_from_slice(content.as_bytes());
    stream.extend_from_slice(b"\nendstream");
    obj(&mut pdf, &stream);
    obj(&mut pdf, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>");
    obj(&mut pdf, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>");
    let mut image = format!(
        "<< /Type /XObject /Subtype /Image /Width {img_w} /Height {img_h} /ColorSpace /DeviceRGB \
         /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
        jpeg.len()
    )
    .into_bytes();
    image.extend_from_slice(jpeg);
    image.extend_from_slice(b"\nendstream");
    obj(&mut pdf, &image);

    let xref_at = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).as_bytes());
    for o in &offsets {
        pdf.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF\n", offsets.len() + 1).as_bytes(),
    );
    pdf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn makes_a_well_formed_pdf() {
        let jpeg = vec![0xFF, 0xD8, 0xFF, 0xD9]; // any bytes: they are copied as-is
        let lines: Vec<String> = (1..=20).map(|i| format!("{i:>3}  Bead number {i} (6mm)")).collect();
        let pdf = design_sheet("My (test) design", "18 in necklace - 16 Nov 2026", &jpeg, 800, 600, &lines);
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.starts_with("%PDF-1.4"));
        assert!(text.trim_end().ends_with("%%EOF"));
        assert!(text.contains("(My \\(test\\) design) Tj"), "brackets are escaped");
        assert!(text.contains("/Filter /DCTDecode"));

        // Every cross-reference entry points at the start of its object (checked on the raw bytes).
        let find = |hay: &[u8], needle: &[u8]| hay.windows(needle.len()).rposition(|w| w == needle).unwrap();
        let sx = find(&pdf, b"startxref\n") + "startxref\n".len();
        let xref_at: usize = std::str::from_utf8(&pdf[sx..]).unwrap().lines().next().unwrap().parse().unwrap();
        let tail = std::str::from_utf8(&pdf[xref_at..]).unwrap();
        assert!(tail.starts_with("xref"));
        let entries: Vec<usize> = tail.lines().skip(3).take(7).map(|l| l[..10].parse().unwrap()).collect();
        for (i, off) in entries.iter().enumerate() {
            assert!(pdf[*off..].starts_with(format!("{} 0 obj", i + 1).as_bytes()), "object {}", i + 1);
        }
        // The content stream's /Length matches what's inside it.
        let start = find(&pdf, b"4 0 obj\n<< /Length ") + "4 0 obj\n<< /Length ".len();
        let len_text = std::str::from_utf8(&pdf[start..start + 12]).unwrap();
        let len: usize = len_text.split(' ').next().unwrap().parse().unwrap();
        let first = |hay: &[u8], needle: &[u8]| hay.windows(needle.len()).position(|w| w == needle).unwrap();
        let body_start = start + first(&pdf[start..], b"stream\n") + "stream\n".len();
        assert_eq!(&pdf[body_start + len..body_start + len + 10], b"\nendstream");
    }

    #[test]
    fn non_western_characters_are_replaced() {
        assert_eq!(pdf_text("71 × 71 px — 6mm"), "71 x 71 px - 6mm");
        assert_eq!(pdf_text("Pérle"), "P?rle");
    }
}
