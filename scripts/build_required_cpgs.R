# =============================================================================
# build_required_cpgs.R
# =============================================================================
# Regenerates ../required_CpGs.csv: the list of CpGs required by every clock the
# beta2clocks app computes (the public methylCIPHER clock set).
#
# Output: one row per unique CpG with columns (CpG, Clocks). The Clocks column
# is a semicolon-separated list of every clock that requires that probe.
#
# Run from a checkout of methylCIPHER (the package supplies the CpG data objects):
#   cd /path/to/MethylCIPHER
#   Rscript /path/to/beta2clocks-app/scripts/build_required_cpgs.R
#
# Requires installed packages: glmnet, EpiDISH, whatsex, DunedinPACE.
# The clock set mirrors clock-metadata.xlsx rows with TranslAGE=="Yes" that are
# NOT MethylCIPHERplus-only (the public build), plus the unconditional setup
# clocks WhatSex and Zhang2019. Update the maps below if that set changes.
# =============================================================================

suppressMessages({ library(glmnet); library(EpiDISH); library(whatsex) })

DATA <- "data"  # methylCIPHER/data
OUT  <- file.path(dirname(dirname(normalizePath(sub("--file=", "",
          grep("--file=", commandArgs(), value = TRUE)[1])))), "required_CpGs.csv")
if (is.na(OUT) || !nzchar(OUT)) OUT <- "required_CpGs.csv"

ld <- function(name) {
  e <- new.env(); load(file.path(DATA, paste0(name, ".rda")), envir = e); get(ls(e)[1], envir = e)
}
onlyCpG <- function(v) { v <- as.character(v); v[grepl("^cg|^ch", v)] }

clocks <- list()

# --- setup clocks (the pipeline runs these unconditionally) ------------------
clocks[["WhatSex"]]   <- get("pprbs", envir = asNamespace("whatsex"))
clocks[["Zhang2019"]] <- ld("Zhang2019_CpGs")$CpG

# --- dataframe-based clocks: name = c(<data object>, <CpG column>) -----------
df_map <- list(
  Hannum         = c("Hannum_CpGs", "Marker"),
  Horvath1       = c("Horvath1_CpGs", "CpGmarker"),
  Horvath2       = c("Horvath2_CpGs", "ID"),
  GrimAgeV1      = c("GrimAgeV1_CpGs", "CpG"),
  GrimAgeV2      = c("GrimAgeV2_CpGs", "CpG"),
  DNAmTL         = c("DNAmTL_CpGs", "ID"),
  PhenoAge       = c("PhenoAge_CpGs", "CpG"),
  AdaptAge       = c("AdaptAge_CpGs", "CpG"),
  CausAge        = c("CausAge_CpGs", "CpG"),
  DamAge         = c("DamAge_CpGs", "CpG"),
  DNAmFI_Li      = c("DNAmFI_Li_CpGs", "CpG"),
  DNAmIC         = c("DNAmIC_CpGs", "CpG"),
  CellDRIFT      = c("CellDRIFT_CpGs", "CpG"),
  CellPopAge     = c("CellPopAge_CpGs", "CpG"),
  DNAmFitAge     = c("DNAmFitAge_CpGs", "CpG"),
  DNAmStress     = c("DNAmStress_CpGs", "CpG"),
  PhysAge        = c("PhysAge_CpGs", "cpg"),
  RepliTali      = c("RepliTali_CpGs", "CpG"),
  RepliTaliNorm  = c("RepliTaliNorm_CpGs", "CpG"),
  RetroAge450K   = c("RetroAge450K_CpGs", "name"),
  RetroAgeEPICv2 = c("RetroAgeEPICv2_CpGs", "name"),
  IntrinClock    = c("IntrinClock_CpGs", "CpG")
)
for (nm in names(df_map)) clocks[[nm]] <- ld(df_map[[nm]][1])[[ df_map[[nm]][2] ]]

# --- plain character-vector clocks -------------------------------------------
clocks[["DunedinPACE"]]   <- ld("DunedinPACE_CpGs")
clocks[["DunedinPoAm38"]] <- ld("DunedinPoAm38_CpGs")
clocks[["PCClocks"]]      <- ld("PCClocks_CpGs")
clocks[["SystemsAge"]]    <- ld("SystemsAge_CpGs")

# --- special cases -----------------------------------------------------------
# StochClocks (StocH/StocP/StocZ) reuse the Horvath/Zhang/PhenoAge CpG panels;
# the CpGs are the rownames of each stochastic glmnet model's beta matrix.
g <- ld("glmStocALL")
clocks[["StochClocks"]] <- unique(unlist(lapply(g, function(x) rownames(x$beta))))
# EpiDISH cell-fraction deconvolution (sentinels Baso/Eos/Neutro/... in pipeline)
data(cent12CT.m); clocks[["EpiDISH_CellFractions"]] <- rownames(cent12CT.m)

# --- assemble long pairs, then collapse to one row per CpG -------------------
pairs <- do.call(rbind, lapply(names(clocks), function(nm) {
  cps <- unique(onlyCpG(clocks[[nm]]))
  if (!length(cps)) return(NULL)
  data.frame(CpG = cps, Clock = nm, stringsAsFactors = FALSE)
}))
# Clocks listed in the deterministic order they appear in `clocks`.
pairs$Clock <- factor(pairs$Clock, levels = names(clocks))
agg <- aggregate(Clock ~ CpG, data = pairs,
                 FUN = function(x) paste(sort(as.integer(x)), collapse = ";"))
# map ordered indices back to names
agg$Clocks <- vapply(strsplit(agg$Clock, ";"), function(ix)
  paste(names(clocks)[as.integer(ix)], collapse = ";"), character(1))
out <- data.frame(CpG = agg$CpG, Clocks = agg$Clocks, stringsAsFactors = FALSE)
out <- out[order(out$CpG), ]
write.csv(out, OUT, row.names = FALSE, quote = FALSE)

cat("Wrote", OUT, "\n")
cat("Unique CpGs (rows):", nrow(out), " Clocks:", length(clocks), "\n")
