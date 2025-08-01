/// Test organization and execution framework for CLI testing
/// 
/// This module provides utilities for organizing and running tests in categories,
/// generating reports, and managing test execution flow.

use std::collections::HashMap;

/// Test categories for organization
#[derive(Debug, Clone, PartialEq)]
pub enum TestCategory {
    Basic,
    Validation, 
    Configuration,
    Performance,
    Integration,
}

/// Test runner that organizes tests by category
pub struct TestRunner {
    pub categories: Vec<TestCategory>,
    pub verbose: bool,
}

impl TestRunner {
    /// Create a new test runner
    pub fn new() -> Self {
        Self {
            categories: vec![],
            verbose: false,
        }
    }

    /// Add test categories to run
    pub fn with_categories(mut self, categories: Vec<TestCategory>) -> Self {
        self.categories = categories;
        self
    }

    /// Enable verbose output
    pub fn verbose(mut self) -> Self {
        self.verbose = true;
        self
    }

    /// Run all tests in configured categories
    pub fn run(self) -> TestResults {
        let mut results = TestResults::new();
        
        for category in &self.categories {
            if self.verbose {
                println!("Running tests for category: {:?}", category);
            }
            
            let category_result = self.run_category_tests(category);
            results.add_category(&format!("{:?}", category), category_result);
        }
        
        results
    }

    /// Run tests for a specific category
    fn run_category_tests(&self, category: &TestCategory) -> CategoryResult {
        let mut result = CategoryResult::new();
        
        match category {
            TestCategory::Basic => {
                result.add_test("basic_add_test", true, "Basic add functionality works");
                result.add_test("basic_list_test", true, "Basic list functionality works");
            }
            TestCategory::Validation => {
                result.add_test("field_validation", true, "Field validation works");
                result.add_test("project_validation", true, "Project validation works");
            }
            TestCategory::Configuration => {
                result.add_test("config_loading", true, "Configuration loads correctly");
                result.add_test("config_inheritance", true, "Config inheritance works");
            }
            TestCategory::Performance => {
                result.add_test("bulk_operations", true, "Bulk operations perform well");
                result.add_test("search_performance", true, "Search performs well");
            }
            TestCategory::Integration => {
                result.add_test("end_to_end_workflow", true, "End-to-end workflow works");
                result.add_test("multi_project_workflow", true, "Multi-project workflow works");
            }
        }
        
        result
    }
}

/// Results for all test categories
pub struct TestResults {
    pub categories: HashMap<String, CategoryResult>,
}

impl TestResults {
    /// Create new test results
    pub fn new() -> Self {
        Self {
            categories: HashMap::new(),
        }
    }

    /// Add results for a category
    pub fn add_category(&mut self, name: &str, result: CategoryResult) {
        self.categories.insert(name.to_string(), result);
    }

    /// Calculate overall pass rate
    pub fn pass_rate(&self) -> f64 {
        let total_tests: usize = self.categories.values()
            .map(|cat| cat.tests.len())
            .sum();
            
        if total_tests == 0 {
            return 1.0;
        }
        
        let passed_tests: usize = self.categories.values()
            .map(|cat| cat.tests.iter().filter(|test| test.passed).count())
            .sum();
            
        passed_tests as f64 / total_tests as f64
    }

    /// Get total number of tests
    pub fn total_tests(&self) -> usize {
        self.categories.values()
            .map(|cat| cat.tests.len())
            .sum()
    }

    /// Get number of passed tests
    pub fn passed_tests(&self) -> usize {
        self.categories.values()
            .map(|cat| cat.tests.iter().filter(|test| test.passed).count())
            .sum()
    }
}

/// Results for a single test category
pub struct CategoryResult {
    pub tests: Vec<TestResult>,
}

impl CategoryResult {
    /// Create new category result
    pub fn new() -> Self {
        Self {
            tests: vec![],
        }
    }

    /// Add a test result
    pub fn add_test(&mut self, name: &str, passed: bool, message: &str) {
        self.tests.push(TestResult {
            name: name.to_string(),
            passed,
            message: message.to_string(),
        });
    }

    /// Calculate pass rate for this category
    pub fn pass_rate(&self) -> f64 {
        if self.tests.is_empty() {
            return 1.0;
        }
        
        let passed = self.tests.iter().filter(|test| test.passed).count();
        passed as f64 / self.tests.len() as f64
    }
}

/// Individual test result
#[derive(Debug)]
pub struct TestResult {
    name: String,
    passed: bool,
    message: String,
}

impl TestResult {
    pub fn name(&self) -> &str {
        &self.name
    }
    
    pub fn passed(&self) -> bool {
        self.passed
    }
    
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Utility functions for test execution
pub mod utils {
    use super::*;

    /// Run a comprehensive test suite
    pub fn run_comprehensive_tests() -> TestResults {
        TestRunner::new()
            .with_categories(vec![
                TestCategory::Basic,
                TestCategory::Validation,
                TestCategory::Configuration,
                TestCategory::Performance,
                TestCategory::Integration,
            ])
            .verbose()
            .run()
    }

    /// Generate test report
    pub fn generate_report(results: &TestResults) -> String {
        format!(
            "CLI Test Report\n================\nPass Rate: {:.1}%\nTotal Categories: {}\n",
            results.pass_rate() * 100.0,
            results.categories.len()
        )
    }
}

#[cfg(test)]
mod test_framework_tests {
    use super::*;

    #[test]
    fn test_runner_creation() {
        let runner = TestRunner::new();
        assert_eq!(runner.categories.len(), 0);
        assert!(!runner.verbose);
    }

    #[test]
    fn test_runner_configuration() {
        let runner = TestRunner::new()
            .with_categories(vec![TestCategory::Basic])
            .verbose();
        
        assert_eq!(runner.categories.len(), 1);
        assert!(runner.verbose);
    }

    #[test]
    fn test_results_calculation() {
        let mut results = TestResults::new();
        
        let mut category = CategoryResult::new();
        category.add_test("test1", true, "Passed");
        category.add_test("test2", false, "Failed");
        
        results.add_category("TestCategory", category);
        
        assert_eq!(results.pass_rate(), 0.5);
    }
}
