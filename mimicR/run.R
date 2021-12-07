library(tidyverse)
rextendr::document()
devtools::load_all(".")


k <- read_delim("https://lhncbc.nlm.nih.gov/ii/tools/MetaMap/Docs/SemanticTypes_2018AB.txt", delim = "|", col_names = c("type", "asd", "s")) %>% pull(type)
raw_freetext_mappings = get_freetext_diagnosis_as_snomed_ids_filter_and_write( 
	"/Users/vinayswamy/columbia/coursework/BINFG4003-symbolic-methods/project/mimicR/noteevents_chunk2.csv",  
c("Discharge Diagnosis", "DISCHARGE DIAGNOSIS", "FINAL DIAGNOSES", "Final Diagnoses"),
  "/Users/vinayswamy/columbia/coursework/BINFG4003-symbolic-methods/project/CUI_to_ICD9.RRF",
  k,
  "freetext_mapping_to_CUI_only_rerun_on_chunk.txt",
  FALSE
)
