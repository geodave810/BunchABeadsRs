//! What you have and what it costs: one line per bead, clasp, crimp or pendant,
//! kept in a CSV file (`Inventory.csv`) that also opens in a spreadsheet.
//!
//! ```text
//! name,on_hand,price_each
//! RedJasper_6mmRD,40,0.12
//! CopperToggle15mm,3,1.50
//! ```

use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stock {
    pub on_hand: u32,
    /// Price of one, in your own currency (0 = not entered).
    pub price_each: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Inventory {
    /// Keyed by lower-case name; keeps the name as written too.
    items: BTreeMap<String, (String, Stock)>,
}

/// One line of the costed material list.
#[derive(Clone, Debug, PartialEq)]
pub struct CostLine {
    pub name: String,
    pub needed: usize,
    pub on_hand: u32,
    /// How many more to buy (0 if you have enough).
    pub to_buy: usize,
    pub price_each: f64,
}

impl CostLine {
    pub fn cost(&self) -> f64 {
        self.needed as f64 * self.price_each
    }
    pub fn buy_cost(&self) -> f64 {
        self.to_buy as f64 * self.price_each
    }
}

/// Material-list labels carry "(crimp)", "(clasp)" or "(pendant)"; the inventory uses the plain name.
pub fn base_name(label: &str) -> &str {
    for suffix in [" (crimp)", " (clasp)", " (pendant)"] {
        if let Some(n) = label.strip_suffix(suffix) {
            return n;
        }
    }
    label
}

impl Inventory {
    pub fn get(&self, name: &str) -> Stock {
        self.items.get(&name.to_lowercase()).map(|e| e.1).unwrap_or_default()
    }

    pub fn set(&mut self, name: &str, stock: Stock) {
        if stock == Stock::default() {
            self.items.remove(&name.to_lowercase());
        } else {
            self.items.insert(name.to_lowercase(), (name.to_string(), stock));
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Reads the CSV. Unreadable lines are skipped; a name may contain commas
    /// because the two numbers are taken from the end of the line.
    pub fn from_csv(text: &str) -> Self {
        let mut inv = Inventory::default();
        for line in text.lines().map(str::trim) {
            if line.is_empty() || line.to_lowercase().starts_with("name,") || line.starts_with('#') {
                continue;
            }
            let mut parts = line.rsplitn(3, ',');
            let (Some(price), Some(qty), Some(name)) = (parts.next(), parts.next(), parts.next()) else { continue };
            let name = name.trim().trim_matches('"');
            if name.is_empty() {
                continue;
            }
            let on_hand = qty.trim().parse::<u32>().unwrap_or(0);
            let price_each = price.trim().trim_start_matches(['$', '£', '€']).parse::<f64>().unwrap_or(0.0).max(0.0);
            inv.set(name, Stock { on_hand, price_each });
        }
        inv
    }

    pub fn to_csv(&self) -> String {
        let mut out = String::from("name,on_hand,price_each\n");
        for (name, stock) in self.items.values() {
            out += &format!("{},{},{}\n", name, stock.on_hand, stock.price_each);
        }
        out
    }

    /// Costs a material list of (count, label) lines.
    pub fn cost(&self, materials: &[(usize, String)]) -> Vec<CostLine> {
        materials
            .iter()
            .map(|(count, label)| {
                let name = base_name(label);
                let stock = self.get(name);
                CostLine {
                    name: label.clone(),
                    needed: *count,
                    on_hand: stock.on_hand,
                    to_buy: count.saturating_sub(stock.on_hand as usize),
                    price_each: stock.price_each,
                }
            })
            .collect()
    }
}

/// A shopping list (what to buy for this design) as CSV.
pub fn shopping_list_csv(lines: &[CostLine]) -> String {
    let mut out = String::from("name,need,have,buy,price_each,buy_cost\n");
    for l in lines.iter().filter(|l| l.to_buy > 0) {
        out += &format!(
            "{},{},{},{},{:.2},{:.2}\n",
            base_name(&l.name),
            l.needed,
            l.on_hand,
            l.to_buy,
            l.price_each,
            l.buy_cost()
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_round_trip_and_costs() {
        let text = "name,on_hand,price_each\nRedJasper_6mmRD,40,0.12\n\"Agate, striped 8mm\",3,$0.50\nCrimp1x2,10,0.02\nbad line\n";
        let inv = Inventory::from_csv(text);
        assert_eq!(inv.len(), 3);
        assert_eq!(inv.get("redjasper_6mmrd"), Stock { on_hand: 40, price_each: 0.12 });
        assert_eq!(inv.get("Agate, striped 8mm").on_hand, 3);
        assert_eq!(Inventory::from_csv(&inv.to_csv()), inv);

        let lines = inv.cost(&[
            (28, "RedJasper_6mmRD".into()),
            (10, "Agate, striped 8mm".into()),
            (4, "Crimp1x2 (crimp)".into()),
            (1, "CopperToggle15mm (clasp)".into()),
        ]);
        assert_eq!(lines[0].to_buy, 0);
        assert!((lines[0].cost() - 3.36).abs() < 1e-9);
        assert_eq!(lines[1].to_buy, 7);
        assert!((lines[1].buy_cost() - 3.5).abs() < 1e-9);
        assert_eq!(lines[2].on_hand, 10); // "(crimp)" is ignored when looking it up
        assert_eq!((lines[3].on_hand, lines[3].to_buy, lines[3].price_each), (0, 1, 0.0));

        let shop = shopping_list_csv(&lines);
        assert!(shop.contains("\"Agate, striped 8mm\",10,3,7,0.50,3.50") || shop.contains("Agate, striped 8mm,10,3,7,0.50,3.50"));
        assert!(shop.contains("CopperToggle15mm,1,0,1,0.00,0.00"));
        assert!(!shop.contains("RedJasper"));
    }

    #[test]
    fn zero_entries_are_removed() {
        let mut inv = Inventory::default();
        inv.set("X", Stock { on_hand: 2, price_each: 0.0 });
        inv.set("x", Stock::default());
        assert!(inv.is_empty());
    }
}
