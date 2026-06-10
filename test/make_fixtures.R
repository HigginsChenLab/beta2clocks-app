# Generate small synthetic .RData fixtures for testing preflight.R + the app.
set.seed(42)
dir <- commandArgs(trailingOnly = TRUE)[1]
if (is.na(dir)) dir <- "."
dir.create(dir, showWarnings = FALSE, recursive = TRUE)

mk_beta <- function(n, m) {
  x <- matrix(runif(n * m, 0, 1), nrow = n, ncol = m)
  colnames(x) <- paste0("cg", sprintf("%08d", seq_len(m)))
  rownames(x) <- paste0("s", seq_len(n))
  x
}
mk_pheno <- function(n) {
  data.frame(
    cSAMPLEID = paste0("s", seq_len(n)),
    cAGE = round(runif(n, 20, 80), 1),
    cFEMALE = sample(c(0, 1), n, replace = TRUE),
    stringsAsFactors = FALSE
  )
}

n <- 12; m <- 2000

# 1. VALID — DNAm* + pheno* naming, betas in [0,1], cAGE + cFEMALE present.
DNAmbeta <- mk_beta(n, m); phenoData <- mk_pheno(n)
save(DNAmbeta, phenoData, file = file.path(dir, "valid_cleaned.RData"))

# 2. BROKEN — missing cFEMALE.
DNAmbeta <- mk_beta(n, m); phenoData <- mk_pheno(n); phenoData$cFEMALE <- NULL
save(DNAmbeta, phenoData, file = file.path(dir, "broken_nosex.RData"))

# 3. BROKEN — beta values out of [0,1] (looks like M-values).
DNAmbeta <- mk_beta(n, m) * 10 - 5; phenoData <- mk_pheno(n)
save(DNAmbeta, phenoData, file = file.path(dir, "broken_range.RData"))

# 4. BROKEN — row count mismatch.
DNAmbeta <- mk_beta(n, m); phenoData <- mk_pheno(n - 3)
save(DNAmbeta, phenoData, file = file.path(dir, "broken_rows.RData"))

# 5. BROKEN — bad sex coding (2).
DNAmbeta <- mk_beta(n, m); phenoData <- mk_pheno(n); phenoData$cFEMALE[1] <- 2
save(DNAmbeta, phenoData, file = file.path(dir, "broken_sexcode.RData"))

# 6. VALID via alt names — datMeth/datPheno + Age/Female mapping.
datMeth <- mk_beta(n, m)
datPheno <- data.frame(Age = round(runif(n, 20, 80), 1),
                       Female = sample(c(0, 1), n, replace = TRUE))
save(datMeth, datPheno, file = file.path(dir, "valid_altnames_cleaned.RData"))

cat("Fixtures written to", normalizePath(dir), "\n")
