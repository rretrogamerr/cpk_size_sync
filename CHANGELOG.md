# Changelog

## 0.2.0
- Uses patched `CPK_ITEM` entries where the 3rd and 4th fields are empty.
- Reads the size from the 5th field (index `4`) in the patched table.
- Writes the size into the original table's 5th field (index `4`), matching on the first two string fields.

## 0.1.0
- Uses patched `CPK_ITEM` entries where the suffix (second string field) is empty.
- Reads the size from the 3rd field (index `2`) in the patched table.
- Writes the size into the original table's 5th field (index `4`), matching on the first two string fields.
