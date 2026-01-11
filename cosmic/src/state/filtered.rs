// SPDX-License-Identifier: GPL-3.0-or-later

#[derive(Clone)]
pub struct Filtered<T> {
    unfiltered: Vec<T>,
    filtered_indices: Vec<usize>,
}

impl<T> Filtered<T> {
    pub fn new(items: Vec<T>) -> Self {
        Self {
            filtered_indices: items.iter().enumerate().map(|(index, _)| index).collect(),
            unfiltered: items,
        }
    }

    pub fn unfiltered_len(&self) -> usize {
        self.unfiltered.len()
    }

    pub fn filtered_len(&self) -> usize {
        self.filtered_indices.len()
    }

    pub fn set_filtered_indices(&mut self, indices: Vec<usize>) {
        self.filtered_indices = indices;
    }

    pub fn unfiltered(&self) -> &[T] {
        self.unfiltered.as_slice()
    }

    pub fn filtered_items(&self) -> Vec<&T> {
        self.filtered_indices
            .iter()
            .map(|index| &self.unfiltered[*index])
            .collect()
    }

    pub fn filter<F>(&mut self, filter_fn: F)
    where
        F: Fn(&T) -> bool,
    {
        self.filtered_indices = self
            .unfiltered
            .iter()
            .enumerate()
            .filter_map(|(index, item)| filter_fn(item).then_some(index))
            .collect();
    }

    pub fn update_item<F>(&mut self, search_fn: F, item: T)
    where
        F: FnMut(&&mut T) -> bool,
    {
        if let Some(element) = self.unfiltered.iter_mut().find(search_fn) {
            *element = item
        }
    }

    pub fn sort_unfiltered<F>(&mut self, sort_fn: F)
    where
        F: FnOnce(&mut [T]),
    {
        sort_fn(&mut self.unfiltered);
        // Reset filtered indices to include all items after sorting
        self.filtered_indices = (0..self.unfiltered.len()).collect();
    }
}
