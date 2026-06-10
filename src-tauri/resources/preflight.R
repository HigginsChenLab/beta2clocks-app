# =============================================================================
# preflight.R  —  fast, base-R-only spot check for the beta2clocks app
# =============================================================================
# Mirrors the data-integrity gating in the beta2clocks pipeline
# (pipeline/entrypoint.R lines ~106-188) WITHOUT loading methylCIPHER or
# computing any clocks. It loads the user's .RData, validates structure, and
# prints exactly one machine-readable line for the desktop app to parse:
#
#   PREFLIGHT_JSON={...}
#
# Usage (inside the container):
#   Rscript /home/app/preflight.R --input data/YourFile.RData
# =============================================================================

# -- tiny base-R JSON encoder (no external packages) --------------------------
jesc <- function(x) {
  x <- as.character(x)
  x <- gsub("\\\\", "\\\\\\\\", x)
  x <- gsub('"', '\\\\"', x)
  x <- gsub("\n", "\\\\n", x)
  x <- gsub("\r", "\\\\r", x)
  x <- gsub("\t", "\\\\t", x)
  x
}
jstr  <- function(x) if (is.null(x) || length(x) == 0 || is.na(x)) "null" else paste0('"', jesc(x), '"')
jbool <- function(x) if (isTRUE(x)) "true" else "false"
jnum  <- function(x) if (is.null(x) || length(x) == 0 || is.na(x)) "null" else format(x, scientific = FALSE, trim = TRUE)
jarr_str <- function(v) if (length(v) == 0) "[]" else paste0("[", paste(vapply(v, jstr, ""), collapse = ","), "]")

checks <- list()
add_check <- function(name, pass, message) {
  checks[[length(checks) + 1]] <<- list(name = name, pass = isTRUE(pass), message = message)
}

emit <- function(ok, beta_var = NULL, pheno_var = NULL, n_samples = NULL,
                 n_cpgs = NULL, array_type = NULL, na_pct = NULL) {
  checks_json <- if (length(checks) == 0) "[]" else paste0(
    "[",
    paste(vapply(checks, function(c) {
      paste0('{"name":', jstr(c$name), ',"pass":', jbool(c$pass), ',"message":', jstr(c$message), "}")
    }, ""), collapse = ","),
    "]"
  )
  errors <- vapply(Filter(function(c) !isTRUE(c$pass), checks), function(c) c$message, "")
  out <- paste0(
    '{"ok":', jbool(ok),
    ',"beta_var":', jstr(beta_var),
    ',"pheno_var":', jstr(pheno_var),
    ',"n_samples":', jnum(n_samples),
    ',"n_cpgs":', jnum(n_cpgs),
    ',"array_type":', jstr(array_type),
    ',"na_pct":', jnum(na_pct),
    ',"checks":', checks_json,
    ',"errors":', jarr_str(errors),
    "}"
  )
  cat(paste0("PREFLIGHT_JSON=", out, "\n"))
  quit(save = "no", status = 0)
}

array_type_from_cpgs <- function(m) {
  if (m < 500000) "450K" else if (m <= 900000) "EPIC v1 (850K)" else "EPIC v2 (935K)"
}

# -- parse --input ------------------------------------------------------------
args <- commandArgs(trailingOnly = TRUE)
ii <- which(args == "--input")
if (length(ii) != 1 || length(args) <= ii) {
  add_check("Input file", FALSE, "No --input path was provided.")
  emit(FALSE)
}
input_path <- args[ii + 1]

if (!file.exists(input_path)) {
  add_check("Input file", FALSE, paste0("File not found: ", input_path))
  emit(FALSE)
}

# -- load ---------------------------------------------------------------------
e <- new.env(parent = emptyenv())
loaded <- tryCatch(load(input_path, envir = e), error = function(err) {
  add_check("Load .RData", FALSE, paste0("Could not read the file as .RData: ", conditionMessage(err)))
  emit(FALSE)
})
vars <- ls(e)
add_check("Load .RData", TRUE, paste0("Loaded variables: ", paste(vars, collapse = ", ")))

# -- find beta matrix (datMeth or ^DNAm), exactly one -------------------------
if ("datMeth" %in% vars) {
  beta_var <- "datMeth"
} else {
  hit <- grep("^DNAm", vars, value = TRUE)
  if (length(hit) != 1) {
    add_check("Beta matrix", FALSE,
              paste0("Expected one variable named 'datMeth' or starting with 'DNAm'. Found: ",
                     paste(vars, collapse = ", ")))
    emit(FALSE)
  }
  beta_var <- hit
}
datMeth <- get(beta_var, envir = e)
add_check("Beta matrix", TRUE, paste0("Found beta matrix '", beta_var, "'."))

# -- find pheno (datPheno or ^pheno), exactly one -----------------------------
if ("datPheno" %in% vars) {
  pheno_var <- "datPheno"
} else {
  hit <- grep("^pheno", vars, value = TRUE)
  if (length(hit) != 1) {
    add_check("Phenotype table", FALSE,
              paste0("Expected one variable named 'datPheno' or starting with 'pheno'. Found: ",
                     paste(vars, collapse = ", ")))
    emit(FALSE)
  }
  pheno_var <- hit
}
datPheno <- get(pheno_var, envir = e)
add_check("Phenotype table", TRUE, paste0("Found phenotype table '", pheno_var, "'."))

# -- dimensions ---------------------------------------------------------------
n_samples <- nrow(datMeth)
n_cpgs    <- ncol(datMeth)
if (is.null(n_samples) || is.null(n_cpgs)) {
  add_check("Beta matrix shape", FALSE, "Beta matrix has no rows/columns dimensions.")
  emit(FALSE, beta_var, pheno_var)
}
array_type <- array_type_from_cpgs(n_cpgs)

# -- row counts match ---------------------------------------------------------
if (nrow(datPheno) != n_samples) {
  add_check("Row counts match", FALSE,
            paste0("Phenotype table has ", nrow(datPheno), " rows but beta matrix has ",
                   n_samples, " samples. They must match (and be in the same order)."))
  emit(FALSE, beta_var, pheno_var, n_samples, n_cpgs, array_type)
}
add_check("Row counts match", TRUE,
          paste0(n_samples, " samples in both the beta matrix and phenotype table."))

# -- numeric ------------------------------------------------------------------
is_num <- if (is.data.frame(datMeth)) all(vapply(datMeth, is.numeric, logical(1))) else is.numeric(datMeth)
if (!is_num) {
  add_check("Beta values numeric", FALSE, "The beta matrix is not numeric.")
  emit(FALSE, beta_var, pheno_var, n_samples, n_cpgs, array_type)
}
add_check("Beta values numeric", TRUE, "Beta matrix is numeric.")

# -- [0,1] spot check (sample up to 10k values) -------------------------------
flat <- if (is.data.frame(datMeth)) as.matrix(datMeth[, sample(ncol(datMeth), min(50, ncol(datMeth))), drop = FALSE]) else datMeth
sample_vals <- suppressWarnings(as.numeric(flat[sample(length(flat), min(10000, length(flat)))]))
sample_vals <- sample_vals[!is.na(sample_vals)]
if (length(sample_vals) > 0 && any(sample_vals < 0 | sample_vals > 1)) {
  add_check("Beta values in [0,1]", FALSE,
            "Some beta values fall outside [0,1]. These should be methylation beta values, not M-values or percentages.")
  emit(FALSE, beta_var, pheno_var, n_samples, n_cpgs, array_type)
}
add_check("Beta values in [0,1]", TRUE, "Sampled beta values are within [0,1].")

# -- NA percentage (informational) --------------------------------------------
na_pct <- tryCatch(round(sum(is.na(datMeth)) / length(datMeth) * 100, 3), error = function(e) NA_real_)

# -- cAGE ---------------------------------------------------------------------
pn <- names(datPheno)
if (!"cAGE" %in% pn) {
  alt <- intersect(c("Age", "age"), pn)
  if (length(alt) > 0) {
    datPheno$cAGE <- datPheno[[alt[1]]]
    add_check("Age column", TRUE, paste0("No 'cAGE'; using '", alt[1], "' as age."))
  } else {
    add_check("Age column", FALSE, "Missing required column 'cAGE' (also looked for 'Age'/'age').")
    emit(FALSE, beta_var, pheno_var, n_samples, n_cpgs, array_type, na_pct)
  }
} else {
  add_check("Age column", TRUE, "Found 'cAGE'.")
}
age_vals <- datPheno$cAGE[!is.na(datPheno$cAGE)]
if (length(age_vals) > 0) {
  age_num <- suppressWarnings(as.numeric(as.character(age_vals)))
  if (any(is.na(age_num))) {
    add_check("Age values valid", FALSE, "'cAGE' contains non-numeric values.")
    emit(FALSE, beta_var, pheno_var, n_samples, n_cpgs, array_type, na_pct)
  }
  if (any(age_num < 0)) {
    add_check("Age values valid", FALSE, "'cAGE' contains negative values.")
    emit(FALSE, beta_var, pheno_var, n_samples, n_cpgs, array_type, na_pct)
  }
  add_check("Age values valid", TRUE, "Age values are numeric and non-negative.")
} else {
  add_check("Age values valid", TRUE, "All ages are NA (will fall back to a DNAm age estimate).")
}

# -- cFEMALE ------------------------------------------------------------------
if (!"cFEMALE" %in% pn) {
  alt <- intersect(c("Female", "FEMALE"), pn)
  if (length(alt) > 0) {
    datPheno$cFEMALE <- datPheno[[alt[1]]]
    add_check("Sex column", TRUE, paste0("No 'cFEMALE'; using '", alt[1], "' as sex."))
  } else {
    add_check("Sex column", FALSE, "Missing required column 'cFEMALE' (also looked for 'Female'/'FEMALE').")
    emit(FALSE, beta_var, pheno_var, n_samples, n_cpgs, array_type, na_pct)
  }
} else {
  add_check("Sex column", TRUE, "Found 'cFEMALE'.")
}
sex_vals <- datPheno$cFEMALE[!is.na(datPheno$cFEMALE)]
if (length(sex_vals) > 0) {
  sex_num <- suppressWarnings(as.numeric(as.character(sex_vals)))
  if (any(is.na(sex_num))) {
    add_check("Sex values valid", FALSE, "'cFEMALE' contains non-numeric values.")
    emit(FALSE, beta_var, pheno_var, n_samples, n_cpgs, array_type, na_pct)
  }
  if (!all(sex_num %in% c(0, 1))) {
    bad <- sort(unique(sex_num[!sex_num %in% c(0, 1)]))
    add_check("Sex values valid", FALSE,
              paste0("'cFEMALE' must be 0 (male) or 1 (female). Found other values: {",
                     paste(bad, collapse = ", "), "}"))
    emit(FALSE, beta_var, pheno_var, n_samples, n_cpgs, array_type, na_pct)
  }
  add_check("Sex values valid", TRUE, "Sex values are coded 0/1.")
} else {
  add_check("Sex values valid", TRUE, "All sex values are NA (will fall back to a predicted sex).")
}

# -- all passed ---------------------------------------------------------------
emit(TRUE, beta_var, pheno_var, n_samples, n_cpgs, array_type, na_pct)
