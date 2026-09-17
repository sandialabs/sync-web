(define-class (ledger)
  ;; Ledger class manages config, staged state, and signed/pinned chain history.
  (define-method (*init* self standard (config '()) tree-class chain-class)
    ;; Initialize ledger with helper objects, inline config data, and fresh storage classes.
    ((self '~field!) 'standard standard)
    ((self '~field!) 'config (expression->byte-vector config))
    (let ((standard-obj (sync-eval standard)))
      ((self '~field!) 'stage ((standard-obj 'init) tree-class))
      ((self '~field!) 'temp ((standard-obj 'init) chain-class))
      ((self '~field!) 'perm ((standard-obj 'init) chain-class))))

  (define-method (~child self node operation . arguments)
    (((sync-eval ((self '~field!) 'standard)) 'deep-call) node '()
     `(lambda (object)
        ((object ',operation)
         ,@(map (lambda (argument) `',argument) arguments)))))

  (define-method (config self (path '()))
    (cond ((and (pair? path) (eq? (car path) 'peer-preapproval))
           ((self '~config-get) (append '(private bridge-preapproval) (cdr path))))
          ((equal? path '(journal-rotation-indexes))
           ((self '~config-get) '(private journal rotation-indexes)))
          ((and (pair? path) (eq? (car path) 'private))
           (error 'config-error "Private Ledger config is not public"))
          (else ((self '~config-get) path))))

  (define-method (descriptor self index)
    ;; Return the public descriptor represented by the selected local state.
    ;; The current implementation publishes descriptor values through committed
    ;; Ledger state; callers use `index` to select the corresponding head.
    (if (not (integer? index))
        (error 'index-error "Descriptor index must be an integer: ~S" index))
    ((self '~config-get) '(public)))

  (define-method (size self)
    ((self '~child) ((self '~field!) 'perm) 'size))

  (define-method (read self index supplied-object (required-paths #f))
    ;; Anchor sparse supplied evidence at an exact permanent-history index and,
    ;; when requested by authenticated orchestration, enrich only exact internal
    ;; paths. Never construct the complete permanent/temporary union.
    (if (or (not (and (sync-node? supplied-object)
                      (sync-pair? supplied-object)))
            (< index 0)
            (>= index ((self 'size)))
            (and required-paths (not (list? required-paths))))
        (error 'integrity-error "Invalid local history object"))
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (local ((self '~child) ((self '~field!) 'perm) 'previous index))
           (window ((self '~config-get) '(public window)))
           (latest (- ((self 'size)) 1))
           (paths (cond ((or (not required-paths) (null? required-paths)) '())
                        ((not (list? (car required-paths)))
                         (list required-paths))
                        (else required-paths)))
           ;; Exact duplicates add no immutable evidence; keep first occurrence.
           (paths
            (let loop ((paths paths) (out '()))
              (if (null? paths) (reverse out)
                  (loop (cdr paths)
                        (if (member (car paths) out)
                            out (cons (car paths) out)))))))
      (if (not (equal? (sync-digest supplied-object) (sync-digest local)))
          (error 'integrity-error "Object is not contained in local history"))
      (let ((temp
             (and (pair? paths)
                  (or (not window) (> index (- latest window)))
                  ((self '~child) ((self '~field!) 'temp)
                   'previous index))))
        (let loop ((result supplied-object) (paths paths))
          (if (null? paths) result
              (let* ((path (car paths))
                     (temp-source?
                      (and temp
                           (equal? (sync-digest local) (sync-digest temp))
                           (not (equal? ((standard 'deep-get) temp path)
                                        '(unknown)))))
                     (source (if temp-source? temp local))
                     (source-known?
                      (not (equal? ((standard 'deep-get) source path)
                                   '(unknown)))))
                (if (and (not source-known?)
                         (equal? ((standard 'deep-get) result path) '(unknown)))
                    (error 'integrity-error
                           "Required history evidence is unavailable: ~S" path))
                (loop
                 (if source-known?
                     ((standard 'deep-merge!) result
                      ((standard 'deep-slice!) source path))
                     result)
                 (cdr paths))))))))

  (define-method (~anchor self index head)
    ;; Anchor a supplied head at the exact index already carried by its
    ;; authenticated committed path; never rediscover it by scanning history.
    (if (not (integer? index))
        (error 'integrity-error "History anchor requires an exact index"))
    ((self 'read) index head))

  (define-method (~identity-verify self identity)
    ;; Validate and return a journal's stable SHA-256 identity commitment.
    (let ((id (and (list? identity) (assoc 'id identity)
                   (cadr (assoc 'id identity))))
          (nonce (and (list? identity) (assoc 'nonce identity)
                      (cadr (assoc 'nonce identity)))))
      (if (not (and (list? identity) (= (length identity) 2)
                    (byte-vector? id) (= (length id) 32)
                    (byte-vector? nonce) (= (length nonce) 32)
                    (equal? id
                            (sync-hash
                             (expression->byte-vector
                              (list 'sync-web/journal-id/v1 nonce))))))
          (error 'identity-error "Invalid journal identity commitment: ~S" identity))
      id))

  (define-method (~rotation-verify self identity rotation previous-key)
    ;; Verify one key transition starting at an already accepted public key.
    (let* ((identity-id ((self '~identity-verify) identity))
           (field (lambda (key) (and (list? rotation) (assoc key rotation)
                                     (cadr (assoc key rotation)))))
           (index (field 'index))
           (previous-index (field 'previous-index))
           (included-previous (field 'previous-key))
           (public-key (field 'public-key))
           (signature (field 'signature)))
      (if (not (and (list? rotation) (= (length rotation) 5)
                    (integer? index) (integer? previous-index)
                    (< previous-index index)
                    (byte-vector? included-previous)
                    (byte-vector? public-key)
                    (byte-vector? signature)))
          (error 'integrity-error
                 "Malformed journal signing-key transition: ~S" rotation))
      (if (not (equal? included-previous previous-key))
          (error 'integrity-error
                 "Journal signing-key transition does not start at the accepted key: ~S"
                 index))
      (if (not (crypto-verify
                previous-key signature
                (expression->byte-vector
                 (list 'sync-web/journal-key-rotation/v1
                       identity-id index previous-index
                       previous-key public-key))))
          (error 'integrity-error
                 "Journal signing-key transition signature does not verify: ~S"
                 index))
      public-key))

  (define-method (~rotate-key! self previous-key public-key signature)
    ;; Stage one root-secret-derived signing-key transition for an atomic step.
    (let* ((identity ((self '~config-get) '(public identity)))
           (configured-key ((self '~config-get) '(public journal public-key)))
           (indexes ((self '~config-get) '(private journal rotation-indexes)))
           (index ((self 'size)))
           (previous-index (if (null? indexes) -1 (car (reverse indexes))))
           (rotation `((index ,index)
                       (previous-index ,previous-index)
                       (previous-key ,previous-key)
                       (public-key ,public-key)
                       (signature ,signature))))
      (if (not (equal? configured-key previous-key))
          (error 'key-rotation-error
                 "Root secret does not match the active journal signing key"))
      (if (not (null? ((self '~config-get) '(private journal pending-rotation))))
          (error 'key-rotation-error
                 "A journal signing-key rotation is already pending commitment"))
      (if (equal? previous-key public-key) #t
          (begin
            ((self '~rotation-verify) identity rotation previous-key)
            ((self '~config-set!) '(private journal pending-rotation) rotation)
            #t))))

  (define-method (~store-peer! self name verified)
    ;; Bind one reciprocal peer using its verified public descriptor.
    (let* ((identity (cadr (assoc 'identity verified)))
           (identity-id (cadr (assoc 'identity-id verified)))
           (public-key (cadr (assoc 'public-key verified)))
           (existing ((self '~config-get) `(private bridge ,name)))
           (bound-identity ((self '~config-get) `(private bridge-identity ,name)))
           (bound-name
            (let find ((bindings ((self '~config-get) '(private bridge-identity))))
              (cond ((null? bindings) #f)
                    ((equal? (cadar bindings) identity-id) (caar bindings))
                    (else (find (cdr bindings)))))))
      (if (not (byte-vector? public-key))
          (error 'bridge-key-error "Bridge descriptor lacks a journal signing key: ~S" name))
      (if (and (not (null? bound-identity))
               (not (equal? bound-identity identity-id)))
          (error 'bridge-name-error
                 "Bridge name is permanently bound to another journal identity: ~S" name))
      (if (and bound-name (not (eq? bound-name name)))
          (error 'bridge-identity-error
                 "Journal identity is permanently bound to another bridge name: ~S" bound-name))
      (if (null? existing)
          (begin
            ((self '~config-set!) `(private bridge-identity ,name) identity-id)
            ((self '~config-set!) `(private bridge ,name identity) identity)
            ((self '~config-set!) `(private bridge ,name public-key) public-key)
            ((self '~config-set!) `(private bridge-retired ,name) '())))
      #t))

  (define-method (get self path)
    (((sync-eval ((self '~field!) 'standard)) 'deep-get)
     ((self '~field!) 'stage) ((self '~path-normalize) path #t #f)))

  (define-method (get-batch self paths)
    ;; Read ordered staged paths from one Ledger snapshot.
    (if (not (list? paths))
        (error 'argument-error "Batch paths must be a proper list"))
    (map (lambda (path) ((self 'get) path)) paths))

  (define-method (set! self path value (expected? #f) (expected #f))
    (if (not (or (byte-vector? value) (equal? value '(nothing))))
        (error 'value-error "Expected bytes or (nothing)"))
    (if (and expected?
             (not (or (byte-vector? expected)
                      (equal? expected '(nothing)))))
        (error 'value-error "Expected comparison bytes or (nothing)"))
    (let ((normalized-path ((self '~path-normalize) path #t #t)))
      (if (and expected? (not (equal? ((self 'get) path) expected))) #f
          (let* ((public-path path)
                 (standard (sync-eval ((self '~field!) 'standard)))
                 (path normalized-path)
                 (stage
                ((standard 'deep-call!) ((self '~field!) 'stage) '()
                 `(lambda (tree)
                    ((tree 'copy!) '(*transition*) '(*transition* previous))
                    ((tree 'set!) '(*transition* operation)
                     '((path ,public-path) (value ,value)))))))
          ((self '~field!) 'stage
           (if (> (length path) 2)
               ((standard 'deep-set!) stage path value)
               ((standard 'deep-call!) stage '()
                `(lambda (tree) ((tree 'set!) ',(car path) ',value)))))))))

  (define-method (set-batch! self changes)
    (if (not (and (list? changes)
                  (let loop ((in changes))
                    (or (null? in)
                        (and (list? (car in))
                             (or
                              (and (= (length (car in)) 2)
                                   (or (byte-vector? (cadr (car in)))
                                       (equal? (cadr (car in)) '(nothing))))
                              (and (= (length (car in)) 3)
                                   (or (byte-vector? (cadr (car in)))
                                       (equal? (cadr (car in)) '(nothing)))
                                   (or (byte-vector? (caddr (car in)))
                                       (equal? (caddr (car in)) '(nothing)))))
                             (loop (cdr in)))))))
        (error 'argument-error "Malformed batch"))
    (for-each
     (lambda (change) ((self '~path-normalize) (car change) #t #t))
     changes)
    (if (let loop ((in changes))
          (and (pair? in)
               (or (and (= (length (car in)) 3)
                        (not (equal? ((self 'get) (caar in))
                                     (caddr (car in)))))
                   (loop (cdr in)))))
        #f
        (begin
          (for-each
           (lambda (change) ((self 'set!) (car change) (cadr change)))
           changes)
          #t)))

  (define-method (~resolve self path pinned? proof? head ancestor?)
    (set! path ((self '~path-normalize) path #f #f))
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (head (if head head ((self '~head) path)))
           (content ((standard 'deep-get) head path))
           (reserved?
            (lambda (name)
              (and (symbol? name)
                   (let ((text (symbol->string name)))
                     (and (> (length text) 1) (char=? (text 0) #\*)
                          (char=? (text (- (length text) 1)) #\*))))))
           (project
            (lambda (value)
              (let loop ((entries (cadr value)) (visible '()))
                (if (null? entries)
                    `(directory ,(reverse visible) ,(caddr value))
                    (loop (cdr entries)
                          (if (reserved? (caar entries)) visible
                              (cons (car entries) visible)))))))
           (directory
            (and ancestor? (pair? content) (eq? (car content) 'directory)
                 (pair? (cdr content)) (list? (cadr content)) content))
           (content
            (cond (directory (project directory))
                  (ancestor? (error 'authorization-error
                                    "Ancestor access requires a directory"))
                  (else content)))
           (proof
            (and proof?
                 (if (not directory) ((standard 'deep-slice!) head path)
                     (let* ((parts (reverse path))
                            (parent (reverse (cdr parts)))
                            (prefix (car parts))
                            (node head))
                       (for-each
                        (lambda (entry)
                          (set! node
                                ((standard 'deep-call!) node parent
                                 `(lambda (tree)
                                    ((tree 'prune!)
                                     ',(append prefix (list (car entry)))
                                     ,(not (reserved? (car entry))))))))
                        (cadr directory))
                       ((standard 'deep-slice!) node path)))))
           (content
            (if (and directory proof?)
                (project ((standard 'deep-get) proof path)) content)))
      (if (or (not (or pinned? proof?))
              (and pinned? (not proof?) (equal? content '(unknown))))
          content
          `((content ,content)
            ,@(if pinned?
                  `((pinned?
                     ,(not (equal?
                            ((standard 'deep-get)
                             ((self '~field!) 'perm) path)
                            '(unknown))))) '())
            ,@(if proof? `((proof ,((standard 'serialize) proof))) '())
            ,@(if ancestor? '((ancestor? #t)) '())))))

  (define-method (resolve-batch self paths (pinned? #f) (heads #f)
                                (ancestors #f) (proof? #f))
    ;; Resolve ordered committed paths and optionally construct one union proof.
    (if (not (and (list? paths)
                  (or (not heads)
                      (and (list? heads) (= (length heads) (length paths))))
                  (or (not ancestors)
                      (and (list? ancestors)
                           (= (length ancestors) (length paths))))))
        (error 'argument-error "Invalid resolve batch shape"))
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (heads (if heads heads (map (lambda (path) #f) paths)))
           (ancestors
            (if ancestors ancestors (map (lambda (path) #f) paths)))
           ;; Ancestor projections require scalar pruning. Exact paths use the
           ;; serializer's native union-of-accesses traversal instead.
           (slice-proof? (and proof? (member #t ancestors))))
      (let loop ((remaining-paths paths)
                 (remaining-heads heads)
                 (remaining-ancestors ancestors)
                 (results '())
                 (proof #f))
        (if (null? remaining-paths)
            (if proof?
                (let* ((head (and (pair? heads) (car heads)))
                       (normalized
                        (and (pair? paths)
                             ((self '~path-normalize) (car paths) #f #f)))
                       (indexed-head?
                        (and (list? head) (assoc 'index head)
                             (assoc 'object head)))
                       (head
                        (cond
                         (indexed-head?
                          ((self '~anchor)
                           (cadr (assoc 'index head))
                           (cadr (assoc 'object head))))
                         (head head)
                         (else #f)))
                       (serialization
                        (cond
                         (proof ((standard 'serialize) proof))
                         ((null? paths) #f)
                         (else ((self 'trace-batch) paths head)))))
                  `((results ,(reverse results))
                    (proof ,serialization)))
                (reverse results))
            (let* ((resolved
                    ((self 'resolve) (car remaining-paths) #f slice-proof?
                     (car remaining-heads) (car remaining-ancestors)))
                   (content
                    (if slice-proof?
                        (cadr (assoc 'content resolved)) resolved))
                   (slice
                    (and slice-proof?
                         ((standard 'deserialize)
                          (cadr (assoc 'proof resolved)))))
                   (proof
                    (cond ((not slice-proof?) #f)
                          ((not proof) slice)
                          (else ((standard 'deep-merge!) slice proof)))))
              (loop
               (cdr remaining-paths) (cdr remaining-heads)
               (cdr remaining-ancestors)
               (cons
                `((content ,content)
                  ,@(if pinned?
                        `((pinned? ,((self 'pinned?)
                                     (car remaining-paths)))) '()))
                results)
               proof))))))

  (define-method (~peer-head self path (index -1) (remote-index -1))
    ;; Describe a committed bridge edge at a selected local/remote index.
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (selected-path
            (if (and (pair? path) (integer? (car path)))
                path (cons index path)))
           (path~ ((self '~path-normalize) selected-path #f #f))
           (segments (cdr selected-path))
           (explicit? (eq? (car segments) '*bridge*))
           (name (if explicit? (cadr segments) (car segments)))
           (local-index (car path~))
           (local-chain
            ((self '~child) ((self '~field!) 'perm) 'previous index))
           (info ((standard 'deep-get) local-chain
                  `(,local-index (*bridge* ,name info))))
           (remote-chain ((standard 'deep-get) local-chain
                          `(,local-index (*bridge* ,name chain))))
           (remote (if (and (sync-node? remote-chain)
                            (not (equal? remote-chain '(nothing)))
                            (not (equal? remote-chain '(unknown)))) remote-chain
                       (error 'bridge-error
                              "Bridge is not committed at the selected local index: ~S ~S"
                              name local-index)))
           (remote-head-index (- ((self '~child) remote 'size) 1))
           (remote-index ((self '~child) remote 'index remote-index))
           (remote-path (if explicit? (cddr segments) (cdr segments))))
      `((interface ,(cadr (assoc 'interface info)))
        (remote-name ,(cadr (assoc 'remote-name info)))
        (identity ,(cadr (assoc 'identity info)))
        (public-key ,(cadr (assoc 'public-key info)))
        (head-index ,remote-head-index)
        (index ,remote-index)
        (path ,remote-path))))

  (define-method (~merge-head self path head (index -1))
    ;; Merge a fetched remote bridge head into the local chain head for a flat path.
    (set! path ((self '~path-normalize) path #f #f))
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (local-chain
            ((self '~child) ((self '~field!) 'perm) 'previous index))
           (remote-chain ((standard 'deep-get) local-chain (list (car path) (cadr path))))
           (remote-path (list-tail path 2))
           (remote-bridge? (and (> (length remote-path) 1)
                                (pair? (cadr remote-path))
                                (eq? (caadr remote-path) '*bridge*)
                                (> (length (cadr remote-path)) 1)))
           (prefix (reverse (list-tail (reverse path) (- (length path) 2)))))
      (if (and (not remote-bridge?)
               (not (equal? (sync-digest remote-chain) (sync-digest head))))
          (error 'integrity-error "Remote chain does not match local bridge head for path: ~S" path)
          ((standard 'deep-merge!) head local-chain prefix))))

  (define-method (~pin-prepare self path proof (head-node #f))
    ;; Anchor or select the immutable history head needed to pin one path.
    (set! path ((self '~path-normalize) path #f #f))
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (window ((self '~config-get) '(public window)))
           (current (- ((self 'size)) 1))
           (target ((self '~child) ((self '~field!) 'perm) 'index (car path))))
      (if (and window (<= target (- current window))) #f
          (let ((head-node
                 (or head-node
                     (if proof
                         (let* ((serialization
                                 (and (list? proof) (assoc 'proof proof)
                                      (cadr (assoc 'proof proof))))
                                (supplied-index
                                 (and (list? proof) (assoc 'index proof)
                                      (cadr (assoc 'index proof)))))
                           (if (not (and serialization (integer? supplied-index)))
                               (error 'integrity-error
                                      "Pinned proof requires an exact history index"))
                           ((self 'read)
                            ((self '~child) ((self '~field!) 'perm)
                             'index supplied-index)
                            ((standard 'deserialize) serialization)))
                         (let* ((head ((self '~head) path))
                                (node ((standard 'deep-get) head path)))
                           (if (or (equal? node '(nothing))
                                   (equal? node '(unknown)))
                               #f
                               ((standard 'deep-slice!) head path)))))))
            (and head-node (list path head-node))))))

  (define-method (~pin-apply! self prepared)
    ;; Merge one already anchored immutable history head into retention state.
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (path (car prepared))
           (head-node (cadr prepared))
           (head-index ((self '~child) head-node 'index (car path)))
           (pinned
            ((standard 'deep-merge!)
             ((self '~child) head-node 'get (car path))
             ((self '~field!) 'perm) `(,head-index)))
           (temp ((self '~field!) 'temp))
           (temp-value ((standard 'deep-get) temp path))
           ;; A selected nested history entry can be absent from the temporary
           ;; origin head even while that origin index is recent.
           (historical-bridge-index?
            (let loop ((segments (cddr path)))
              (and (pair? segments)
                   (or (and (integer? (car segments)) (>= (car segments) 0))
                       (loop (cdr segments)))))))
      ((self '~field!) 'perm pinned)
      (if (and historical-bridge-index?
               (equal? temp-value '(unknown))
               (equal? (sync-digest pinned) (sync-digest temp)))
          ((self '~field!) 'temp
           ((standard 'deep-merge!)
            ((standard 'deep-slice!) pinned path) temp)))
      #t))

  (define-method (pin! self path proof)
    ;; Merge a prepared pinned proof object into the permanent chain.
    ;; A response is anchored against the matching retained local history root,
    ;; so a concurrent later step does not invalidate an already resolved proof.
    (let ((prepared ((self '~pin-prepare) path proof)))
      (and prepared ((self '~pin-apply!) prepared))))

  (define-method (pin-batch! self paths (proofs #f))
    ;; Atomically retain ordered paths with optional prepared proof responses.
    ;; Internal prepared batches carry each route-group proof once plus an exact
    ;; slot vector; ordinary callers retain the aligned proof-list form. Exact
    ;; local no-proof duplicates reuse only a prior successful attempt member.
    (let* ((compact?
            (and (list? proofs) (= (length proofs) 2)
                 (list? (car proofs)) (= (length (car proofs)) 2)
                 (eq? (caar proofs) 'proofs)
                 (list? (cadr proofs)) (= (length (cadr proofs)) 2)
                 (eq? (caadr proofs) 'slots)))
           (groups (and compact? (cadar proofs)))
           (slots (and compact? (cadadr proofs))))
      (if (not (and (list? paths)
                    (or (not proofs)
                        (and compact? (list? groups) (list? slots)
                             (= (length slots) (length paths)))
                        (and (list? proofs)
                             (= (length proofs) (length paths))))))
          (error 'argument-error "Invalid pin batch shape"))
      (if compact?
          (let validate ((slots slots))
            (if (pair? slots)
                (let ((slot (car slots)))
                  (if (not (or (not slot)
                               (and (integer? slot) (>= slot 0)
                                    (< slot (length groups)))))
                      (error 'argument-error "Invalid prepared pin slot"))
                  (validate (cdr slots))))))
      (let ((perm ((self '~field!) 'perm))
            (temp ((self '~field!) 'temp)))
        (let loop ((paths paths)
                   (slots (if compact? slots
                              (if proofs proofs (map (lambda (path) #f) paths))))
                   (heads '())
                   (local-paths '()))
          (if (null? paths) #t
              (let* ((slot (car slots))
                     (proof (if compact? (and slot (list-ref groups slot)) slot))
                     (cached (and proof (assq proof heads)))
                     (local-duplicate
                      (and (not proof) (member (car paths) local-paths))))
                (cond
                 (local-duplicate
                  ;; An exact local path successfully retained earlier in this
                  ;; attempt needs no second immutable preparation or mutation.
                  (loop (cdr paths) (cdr slots) heads local-paths))
                 ((and compact? cached)
                  ;; The exact prepared slot references this already anchored
                  ;; route-group proof. Its union contains every selected path,
                  ;; so ordered duplicate and sibling slots require no repeated
                  ;; deserialize, anchor, or Merkle merge.
                  (loop (cdr paths) (cdr slots) heads local-paths))
                 (else
                  (let ((prepared
                         ((self '~pin-prepare) (car paths) proof
                          (and cached (cdr cached)))))
                    (if prepared
                        (begin
                          (if (not cached) ((self '~pin-apply!) prepared))
                          (loop
                           (cdr paths) (cdr slots)
                           (if (or (not proof) cached) heads
                               (cons (cons proof (cadr prepared)) heads))
                           (if proof local-paths
                               (cons (car paths) local-paths))))
                        (begin
                          ((self '~field!) 'perm perm)
                          ((self '~field!) 'temp temp)
                          #f)))))))))))

  (define-method (unpin! self path)
    ;; Remove a path from the permanent chain.
    (set! path ((self '~path-normalize) path #f #f))
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (perm ((self '~field!) 'perm)))
      ((self '~field!) 'perm ((standard 'deep-prune!) perm path))))

  (define-method (unpin-batch! self paths)
    ;; Atomically remove retained information for each requested path.
    (if (not (list? paths))
        (error 'argument-error "Batch paths must be a proper list"))
    (for-each (lambda (path) ((self 'unpin!) path)) paths)
    #t)

  (define-method (~signed-head self (known-index -1) (known-through -1))
    ;; Serialize the latest signed head and receiver-possible historical digests.
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (perm ((self '~field!) 'perm))
           (rotation-indexes
            (if (< known-index 0) '()
                (let loop
                    ((index
                      ((self '~config-get)
                       '(public journal latest-rotation-index)))
                     (selected '()))
                  (if (or (not (integer? index)) (<= index known-index))
                      selected
                      (let* ((rotation
                              ((standard 'deep-get) perm
                               `(,index (*crypto* journal rotation))))
                             (previous
                              (and (list? rotation)
                                   (assoc 'previous-index rotation)
                                   (cadr (assoc 'previous-index rotation)))))
                        (if (not (and (integer? previous) (< previous index)))
                            (error 'integrity-error
                                   "Missing journal rotation at index: ~S"
                                   index))
                        (loop previous (cons index selected))))))))
      ((standard 'serialize) perm
       `(lambda (node)
          (let ((chain (sync-eval node)))
            (if (> ((chain 'size)) 0)
                (let* ((node ((chain 'get) -1))
                       (tree (sync-eval node)))
                  ((tree 'get) '(*crypto* public-key))
                  ((tree 'get) '(*crypto* signature))
                  ((tree 'get) '(*crypto* journal identity id))
                  ((tree 'get) '(*crypto* journal identity nonce))
                  ((tree 'get) '(*crypto* journal latest-rotation-index))
                  ((tree 'get) '(*crypto* journal public-key))
                  ((tree 'get) '(*crypto* journal signature))
                  ((tree 'get) '(*crypto* interface public-key))
                  ((tree 'get) '(*crypto* interface endpoint))
                  ((tree 'get) '(*crypto* journal name))
                  ,@(map
                     (lambda (rotation-index)
                       `(let* ((rotation-node ((chain 'get) ,rotation-index))
                               (rotation-tree (sync-eval rotation-node)))
                          ((rotation-tree 'get) '(*crypto* journal rotation))))
                     rotation-indexes)
                  ((chain 'digest))
                  ,(if (>= known-index 0)
                       `(if ,(if (>= known-through known-index)
                                 #t `(< ,known-index ((chain 'size))))
                            (begin
                              ((chain 'digest) ,known-index)
                              ,(if (> known-through known-index)
                                   `((chain 'digest) ,known-through)
                                   #t))
                            #t)
                       #t))))))))

  (define-method (~trace self index path head)
    ;; Trace a remote path against a serialized chain at index.
    (set! path ((self '~path-normalize) path #f #f))
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (head (if head head ((self '~head) path index)))
           (last (and (pair? path) (car (reverse path))))
           (boundary? (or (null? path)
                          (and (pair? last) (= (length last) 3)
                               (eq? (car last) '*bridge*)
                               (eq? (caddr last) 'chain)))))
      ((standard 'serialize) head
       `(lambda (node)
          (letrec ((deep-get (lambda (node path)
                               (if (or (null? path)
                                       (equal? node '(nothing))
                                       (equal? node '(unknown))) node
                                   (let ((child (((sync-eval node) 'get) (car path))))
                                     (if (not (sync-node? child)) child
                                         (deep-get child (cdr path))))))))
            (let ((result (deep-get node ',path)))
              ,(if boundary?
                   '(begin (sync-car result) result)
                   'result)))))))

  (define-method (~accept-peer-head! self name response info interface remote-name
                                      known-index verified)
    ;; Structurally apply a Federation-authenticated reciprocal peer head.
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (latest ((standard 'deserialize) response))
           (existing ((self '~config-get) `(private bridge ,name)))
           (latest-size ((self '~child) latest 'size)))
      (if (= latest-size 0)
          (error 'bridge-sync-error
                 "A reciprocal bridge requires a signed peer head: ~S" name))
      (if (null? existing)
          ((self '~store-peer!) name verified))
      ((self '~store-peer-response!)
       name response interface remote-name verified)
      `((ok? #t)
        (accepted-index ,((self '~config-get) `(private bridge ,name last-index)))
        (head-index ,(- ((self 'size)) 1))
        (response ,((self '~signed-head) known-index))
        (info ,((self 'descriptor) -1))
        (interface ,((self '~config-get) '(public interface endpoint))))))

  (define-method (~store-peer-response! self name response interface remote-name verified)
    ;; Structurally validate and stage Federation-authenticated peer evidence.
    (if (and (list? response) (pair? response) (eq? (car response) 'error)) #f
        (let* ((standard (sync-eval ((self '~field!) 'standard)))
               (stage ((self '~field!) 'stage))
               (latest ((standard 'deserialize) response))
               (identity (cadr (assoc 'identity verified)))
               (public-key (cadr (assoc 'public-key verified)))
               (value ((self '~child) stage 'get `(*bridge* ,name chain)))
               (latest-size ((self '~child) latest 'size))
               (last-size (if (sync-node? value) ((self '~child) value 'size) 0))
               (latest-index (if (> latest-size 0) (- latest-size 1) -1))
               (last-index (if (> last-size 0) (- last-size 1) -1)))
          (if (not (and (list? verified)
                        (assoc 'identity verified)
                        (byte-vector? (cadr (assoc 'identity-id verified)))
                        (byte-vector? public-key)
                        (= (cadr (assoc 'index verified)) latest-index)))
              (error 'bridge-sync-error
                     "Malformed authenticated peer evidence: ~S" name))
          (if (null? ((self '~config-get) `(private bridge ,name)))
              (error 'bridge-name-error "Unknown reciprocal bridge: ~S" name))
          (if (= latest-size 0)
              (error 'bridge-sync-error "A reciprocal bridge requires a signed peer head: ~S" name))
          ((self '~config-set!) `(private bridge ,name public-key) public-key)
          (if (< latest-index last-index)
              (error 'bridge-sync-error "Peer head is older than stored bridge head: ~S" name))
          (if (and (>= last-index 0)
                   (not (equal?
                         ((self '~child) value 'digest)
                         ((self '~child) latest 'digest last-index))))
              (error 'bridge-sync-error "Peer head does not continue stored bridge head: ~S" name))
          (if (= latest-index last-index) #t
              (let* ((stored-chain (if (> latest-size 0)
                                       ((standard 'deep-slice!) latest '(-1 ())) latest))
                     (info `((valid? #t)
                             (index ,latest-index)
                             (interface ,interface)
                             (identity ,identity)
                             (public-key ,public-key)
                             (remote-name ,remote-name))))
                (set! stage
                      (sync-let ((stage stage) (name name)
                                 (latest-index latest-index) (info info)
                                 (stored-chain stored-chain))
                        (let ((stage (sync-eval stage)))
                          ((stage 'copy!) '(*transition*) '(*transition* previous))
                          ((stage 'set!) '(*transition* operation)
                           `((function synchronize!)
                             (path (*bridge* ,name))
                             (accepted-index ,latest-index)))
                          ((stage 'set!) `(*bridge* ,name info) info)
                          ((stage 'set!) `(*bridge* ,name chain) stored-chain)
                          (stage))))
                ((self '~config-set!) `(private bridge ,name last-index) latest-index)
                ((self '~field!) 'stage stage))))))

  (define-method (~step! self unix-time public-key secret-key)
    ;; Commit staged changes to permanent chain and update temp window.
    (let* ((window ((self '~config-get) '(public window)))
           (standard (sync-eval ((self '~field!) 'standard)))
           (stage ((self '~field!) 'stage))
           (perm ((self '~field!) 'perm))
           (temp ((self '~field!) 'temp))
           (perm-size ((self '~child) perm 'size))
           (identity ((self '~config-get) '(public identity)))
           (pending-rotation
            ((self '~config-get) '(private journal pending-rotation)))
           (rotation-indexes
            ((self '~config-get) '(private journal rotation-indexes)))
           (configured-key ((self '~config-get) '(public journal public-key)))
           (key-change? (not (equal? configured-key public-key)))
           (curr-digest
            (sync-digest
             ((standard 'deep-call!) stage '()
              '(lambda (obj)
                 ((obj 'set!) '(*transition*) '(nothing))))))
           (prev-digest
            (if (= perm-size 0) (sync-digest (sync-null))
                (sync-digest
                 ((standard 'deep-call!)
                  ((self '~child) perm 'get -1)
                  '()
                  '(lambda (obj)
                     ((obj 'set!) '(*transition*) '(nothing))
                     ((obj 'set!) '(*crypto*) '(nothing))))))))
      (if key-change?
          (let ((expected-previous-index
                 (if (null? rotation-indexes) -1
                     (car (reverse rotation-indexes)))))
            (if (or (null? pending-rotation)
                    (not (= (cadr (assoc 'index pending-rotation))
                            perm-size))
                    (not (= (cadr (assoc 'previous-index pending-rotation))
                            expected-previous-index))
                    (not (equal? (cadr (assoc 'public-key pending-rotation))
                                 public-key)))
                (error 'key-rotation-error
                       "Root secret is not authorized by a pending key transition"))
            ((self '~rotation-verify) identity pending-rotation configured-key))
          (if (not (null? pending-rotation))
              (error 'key-rotation-error
                     "Pending journal signing key does not match the root secret")))
      (if (or key-change? (not (equal? curr-digest prev-digest)))
          (let ((utc-time (system-time-utc unix-time)))
            (set! stage
                  ((standard 'deep-call!) stage '()
                   `(lambda (tree)
                      ((tree 'copy!) '(*transition*) '(*transition* previous))
                      ((tree 'set!) '(*transition* operation)
                       '((path (*state* *time*)) (value ,utc-time)))
                      ((tree 'set!) '(*state* *time*) ,utc-time))))
            (set! perm
                  (sync-let ((perm perm) (stage stage))
                    (let ((perm (sync-eval perm)))
                      ((perm 'push!) stage)
                      (perm))))
            (set! stage
                  ((standard 'deep-call!) stage '()
                   '(lambda (tree) ((tree 'set!) '(*transition*) '(nothing)))))
            (set! perm
                  ((self '~signature-sign!)
                   perm public-key secret-key pending-rotation))
            ((self '~config-set!) '(public public-key) public-key)
            ((self '~config-set!) '(public journal public-key) public-key)
            (if key-change?
                (begin
                  ((self '~config-set!) '(private journal rotation-indexes)
                   (append rotation-indexes
                           (list (cadr (assoc 'index pending-rotation)))))
                  ((self '~config-set!) '(public journal latest-rotation-index)
                   (cadr (assoc 'index pending-rotation)))
                  ((self '~config-set!) '(private journal pending-rotation) '())))
            (let ((latest ((self '~child) perm 'get -1)))
              (set! temp
                    (sync-let ((temp temp) (latest latest))
                      (let ((temp (sync-eval temp)))
                        ((temp 'push!) latest)
                        (temp)))))
            (let ((time-node ((standard 'deep-slice!) temp '(-1 (*state* *time*)))))
              (if (and window (> ((self '~child) temp 'size) window))
                  (let ((truncate-index
                         (- ((self '~child) temp 'size) window 1)))
                    (set! temp
                          ((standard 'deep-call!) temp '()
                           `(lambda (chain)
                              ((chain 'truncate!) ,truncate-index))))))
              ((self '~field!) 'stage stage)
              ((self '~field!) 'perm
               ((standard 'deep-merge!) time-node
                ((standard 'deep-prune!) perm '(-1 (*state*)))))
              ((self '~field!) 'temp temp))))
      ((self '~child) perm 'size)))

  (define-method (~update-config! self path value)
    ;; Update a config entry in place. Shrinking the public retention window
    ;; destructively prunes newly excluded temporary history.
    (if (equal? path '(public window))
        (let* ((standard (sync-eval ((self '~field!) 'standard)))
               (temp ((self '~field!) 'temp))
               (old-window ((self '~config-get) '(public window)))
               (size ((self '~child) temp 'size)))
          (if (and (integer? value) (> size value)
                   (or (not (integer? old-window)) (< value old-window)))
              (set! temp
                    ((standard 'deep-call!) temp '()
                     `(lambda (chain)
                        ((chain 'truncate!) ,(- size value 1))))))
          ((self '~field!) 'temp temp)))
    ((self '~config-set!) path value))

  ;; Public data/history protocol used by Interface and Federation.

  (define-method (resolve self path (pinned? #f) (proof? #f)
                          (head #f) (ancestor? #f))
    (let* ((normalized ((self '~path-normalize) path #f #f))
           (indexed-head?
            (and (list? head) (assoc 'index head) (assoc 'object head)))
           (head-index
            (if indexed-head? (cadr (assoc 'index head))
                ((self '~child) ((self '~field!) 'perm)
                 'index (car normalized))))
           (head-object
            (if indexed-head? (cadr (assoc 'object head)) head)))
      ((self '~resolve) path pinned? proof?
       (and head-object ((self '~anchor) head-index head-object)) ancestor?)))

  (define-method (trace self path (head #f))
    (if (not head)
        ((self '~trace)
         (if (and (pair? path) (integer? (car path))) (car path) -1)
         (if (and (pair? path) (integer? (car path))) (cdr path) path)
         #f)
        (let* ((standard (sync-eval ((self '~field!) 'standard)))
               (normalized ((self '~path-normalize) path #f #f)))
          ((standard 'serialize)
           ((standard 'deep-slice!) head normalized)))))

  (define-method (trace-batch self paths (head #f))
    ;; Serialize the union of accesses for paths sharing one authenticated head.
    (if (not (and (list? paths) (pair? paths)))
        (error 'argument-error "Trace batch requires at least one path"))
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (normalized (map (lambda (path)
                              ((self '~path-normalize) path #f #f))
                            paths))
           (trace-items
            (map
             (lambda (path)
               (let ((last (and (pair? path) (car (reverse path)))))
                 (list
                  path
                  (or (null? path)
                      (and (pair? last) (= (length last) 3)
                           (eq? (car last) '*bridge*)
                           (eq? (caddr last) 'chain))))))
             normalized))
           (indexes (map car normalized))
           (index (car indexes)))
      (if (not (let loop ((indexes (cdr indexes)))
                 (or (null? indexes)
                     (and (= (car indexes) index)
                          (loop (cdr indexes))))))
          (error 'index-error "Trace batch paths require one history anchor"))
      (let ((head
             (if head head
                 (let loop ((paths normalized) (proof #f))
                   (if (null? paths) proof
                       (let* ((source ((self '~head) (car paths)))
                              (slice
                               ((standard 'deep-slice!) source (car paths))))
                         (loop
                          (cdr paths)
                          (if proof
                              ((standard 'deep-merge!) slice proof)
                              slice))))))))
        ((standard 'serialize) head
         `(lambda (node)
            (letrec ((deep-get
                      (lambda (node path)
                        (if (or (null? path)
                                (equal? node '(nothing))
                                (equal? node '(unknown))) node
                            (let ((child (((sync-eval node) 'get) (car path))))
                              (if (not (sync-node? child)) child
                                  (deep-get child (cdr path))))))))
              (for-each
               (lambda (item)
                 (let ((result (deep-get node (car item))))
                   (if (cadr item) (sync-car result) result)))
               ',trace-items)))))))

  (define-method (pinned? self path)
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (normalized ((self '~path-normalize) path #f #f)))
      (not (equal? ((standard 'deep-get)
                    ((self '~field!) 'perm) normalized)
                   '(unknown)))))

  (define-method (signed-head self known-index)
    (cond ((integer? known-index) ((self '~signed-head) known-index -1))
          ((and (list? known-index) (assoc 'index known-index))
           ((self '~signed-head) (cadr (assoc 'index known-index))
            (if (assoc 'through known-index)
                (cadr (assoc 'through known-index)) -1)))
          (else (error 'index-error "Invalid signed-head cursor"))))

  (define-method (peer-head self alias index)
    (if (and (symbol? alias) (list? index) (assoc 'checkpoint index))
        ((self '~peer-checkpoint) alias)
        (let* ((path (if (list? alias) alias `(*bridge* ,alias)))
               (local-index
                (if (and (list? index) (assoc 'local index))
                    (cadr (assoc 'local index)) index))
               (remote-index
                (if (and (list? index) (assoc 'remote index))
                    (cadr (assoc 'remote index)) -1)))
          ((self '~peer-head) path local-index remote-index))))

  (define-method (~peer-checkpoint self alias)
    ;; Return inert accepted peer evidence and an opaque apply precondition.
    (let* ((active ((self '~config-get) `(private bridge ,alias)))
           (retired ((self '~config-get) `(private bridge-retired ,alias)))
           (binding ((self '~config-get) `(private bridge-identity ,alias)))
           (preapproval
            ((self '~config-get) `(private bridge-preapproval ,alias)))
           (selected (if (null? active) retired active))
           (get (lambda* (key (default '()))
                  (let ((entry (and (list? selected) (assoc key selected))))
                    (if entry (cadr entry) default))))
           (status (cond ((not (null? active)) 'active)
                         ((not (null? retired)) 'retired) (else 'absent)))
           (head ((self '~child) ((self '~field!) 'stage)
                  'get `(*bridge* ,alias chain)))
           (head-digest (if (sync-node? head) (sync-digest head) '()))
           (body
            `((status ,status)
              (identity ,(get 'identity))
              (identity-id ,binding)
              (public-key ,(get 'public-key))
              (accepted-index ,(get 'last-index -1))
              (head-digest ,head-digest)
              (acceptance ,((self '~config-get) '(public bridge-accept)))
              (preapproval ,preapproval))))
      (append body
              `((checkpoint
                 ,(sync-hash (expression->byte-vector body)))))))

  (define-method (merge-head! self alias supplied-head)
    (let* ((path (cond ((and (list? alias) (assoc 'path alias))
                        (cadr (assoc 'path alias)))
                       ((list? alias) alias)
                       (else `(*bridge* ,alias))))
           (head (if (and (list? supplied-head) (assoc 'head supplied-head))
                     (cadr (assoc 'head supplied-head)) supplied-head))
           (index (cond ((and (list? alias) (assoc 'index alias))
                         (cadr (assoc 'index alias)))
                        ((and (list? supplied-head) (assoc 'index supplied-head))
                         (cadr (assoc 'index supplied-head)))
                        (else -1))))
      ((self '~merge-head) path head index)))

  (define-method (store-peer-head! self alias verified-head)
    (if (not (and (symbol? alias) (list? verified-head)
                  (assoc 'mode verified-head)))
        (error 'bridge-sync-error
               "Malformed prepared peer-head update: ~S" verified-head))
    (if (and (assoc 'checkpoint verified-head)
             (not (equal?
                   (cadr (assoc 'checkpoint verified-head))
                   (cadr (assoc 'checkpoint
                                ((self '~peer-checkpoint) alias))))))
        (error 'federation-conflict
               "Peer checkpoint changed before durable apply: ~S" alias))
    (case (cadr (assoc 'mode verified-head))
      ((accept)
       ((self '~accept-peer-head!)
        alias
        (cadr (assoc 'response verified-head))
        (and (assoc 'info verified-head)
             (cadr (assoc 'info verified-head)))
        (cadr (assoc 'interface verified-head))
        (cadr (assoc 'remote-name verified-head))
        (if (assoc 'known-index verified-head)
            (cadr (assoc 'known-index verified-head)) -1)
        (cadr (assoc 'verified verified-head))))
      ((update)
       ((self '~store-peer-response!)
        alias (cadr (assoc 'response verified-head))
        (cadr (assoc 'interface verified-head))
        (cadr (assoc 'remote-name verified-head))
        (cadr (assoc 'verified verified-head))))
      ((establish)
       ((self '~store-peer!) alias (cadr (assoc 'verified verified-head)))
       ((self '~store-peer-response!)
        alias
        (cadr (assoc 'response verified-head))
        (cadr (assoc 'interface verified-head))
        (cadr (assoc 'remote-name verified-head))
        (cadr (assoc 'verified verified-head))))
      (else
       (error 'bridge-sync-error
              "Unknown prepared peer-head update mode: ~S"
              (cadr (assoc 'mode verified-head))))))

  (define-method (step! self prepared-inputs)
    (if (not (and (list? prepared-inputs)
                  (assoc 'unix-time prepared-inputs)
                  (assoc 'public-key prepared-inputs)
                  (assoc 'secret-key prepared-inputs)))
        (error 'argument-error "Malformed prepared Ledger step: ~S"
               prepared-inputs))
    (if (assoc 'rotation prepared-inputs)
        (let ((rotation (cadr (assoc 'rotation prepared-inputs))))
          ((self '~rotate-key!)
           (cadr (assoc 'previous-key rotation))
           (cadr (assoc 'public-key rotation))
           (cadr (assoc 'signature rotation)))))
    ((self '~step!)
     (cadr (assoc 'unix-time prepared-inputs))
     (cadr (assoc 'public-key prepared-inputs))
     (cadr (assoc 'secret-key prepared-inputs))))

  (define-method (update-config! self changes)
    (if (and (equal? (cadr (assoc 'path changes)) '(public window))
             (not (or (not (cadr (assoc 'value changes)))
                      (and (integer? (cadr (assoc 'value changes)))
                           (> (cadr (assoc 'value changes)) 0)))))
        (error 'argument-error "Window must be positive"))
    (if (not (and (list? changes) (assoc 'path changes)
                  (assoc 'value changes)))
        (error 'argument-error "Malformed Ledger config change: ~S" changes))
    ((self '~update-config!)
     (cadr (assoc 'path changes)) (cadr (assoc 'value changes))))

  (define-method (~field! self name value)
    ;; Resolve or set internal field by name.
    (let ((address (case name
                     ((standard) '(1 0 0 0))
                     ((config) '(1 0 0 1))
                     ((stage) '(1 0 1 0))
                     ((temp) '(1 0 1 1))
                     ((perm) '(1 1))
                     (else (error 'field-error "Ledger field not found: ~S" name)))))
      (if value (set! (self address) value)
          (self address))))

  (define-method (~path-normalize self path stage? state-only?)
    ;; Convert a flat public path into the current nested ledger representation.
    (define (reject reason)
      (error 'path-error "Invalid ledger path (~A): ~S" reason path))
    (define (contains-pair? xs)
      (and (pair? xs)
           (or (pair? (car xs)) (contains-pair? (cdr xs)))))
    (define (indexed-tail segments)
      (cond ((null? segments) '())
            ((integer? (car segments)) (indexed-segments (car segments) (cdr segments)))
            (else (indexed-segments -1 segments))))
    (define (indexed-segments index segments)
      (cond ((null? segments) `(,index))
            ((eq? (car segments) '*state*) `(,index ,segments))
            ((eq? (car segments) '*transition*) `(,index ,segments))
            ((eq? (car segments) '*crypto*) `(,index ,segments))
            ;; `*bridge*` remains the local bridge namespace and the explicit
            ;; compatibility form. Concise paths name each bridge directly and
            ;; optionally follow it with the selected remote index.
            ((eq? (car segments) '*bridge*)
             (cond ((null? (cdr segments)) `(,index (*bridge*)))
                   ((null? (cddr segments)) `(,index (*bridge* ,(cadr segments) chain)))
                   (else (append `(,index (*bridge* ,(cadr segments) chain))
                                 (indexed-tail (cddr segments))))))
            ((symbol? (car segments))
             (let* ((name (car segments))
                    (rest (cdr segments))
                    (remote-index (if (and (pair? rest) (integer? (car rest)))
                                      (car rest) -1))
                    (tail (if (and (pair? rest) (integer? (car rest)))
                              (cdr rest) rest)))
               (append `(,index (*bridge* ,name chain))
                       (indexed-segments remote-index tail))))
            (else (reject "expected bridge name or namespace marker"))))
    (cond ((not (list? path)) (reject "not a list"))
          ((contains-pair? path) (reject "nested public paths are not supported"))
          (stage?
           (cond ((or (null? path) (integer? (car path))) (reject "stage paths cannot start with an index"))
                 ((and state-only? (not (eq? (car path) '*state*))) (reject "expected *state*"))
                 ((eq? (car path) '*state*) `(,path))
                 ((and (not state-only?) (eq? (car path) '*transition*)) `(,path))
                 ((and (not state-only?) (eq? (car path) '*bridge*))
                  (cond ((null? (cdr path)) '((*bridge*)))
                        ((null? (cddr path)) `((*bridge* ,(cadr path) chain)))
                        (else (reject "stage bridge traversal is not supported"))))
                 (else (reject "expected namespace marker"))))
          (else (indexed-tail path))))

  (define-method (delete-peer-head! self alias)
    ;; Remove active peer state while retaining permanent identity binding.
    (let ((existing ((self '~config-get) `(private bridge ,alias))))
      (if (not (null? existing))
          ((self '~config-set!) `(private bridge-retired ,alias)
           `((identity ,(cadr (assoc 'identity existing)))
             (public-key ,(cadr (assoc 'public-key existing)))
             (last-index ,(cadr (assoc 'last-index existing))))))
      ((self '~config-set!) `(private bridge ,alias) '())
      ((self '~config-set!) `(private bridge-preapproval ,alias) '())
      ((self '~field!) 'stage
       (((sync-eval ((self '~field!) 'standard)) 'deep-call!)
        ((self '~field!) 'stage) '()
        `(lambda (tree) ((tree 'set!) '(*bridge* ,alias) '(nothing)))))))

  (define-method (~config-get self (path '()))
    (let loop ((config (byte-vector->expression ((self '~field!) 'config))) (path path))
      (if (null? path) config
          (let ((match (assoc (car path) config)))
            (if (not match) '()
                (loop (cadr match) (cdr path)))))))

  (define-method (~config-set! self (path '()) value)
    ((self '~field!) 'config
     (expression->byte-vector
      (let loop-1 ((config (byte-vector->expression ((self '~field!) 'config))) (path path))
        (if (null? path) value
            (let loop-2 ((config config))
              (cond ((null? config)
                     (if (eq? value '()) '()
                         (list (list (car path) (loop-1 '() (cdr path))))))
                    ((eq? (caar config) (car path))
                     (let ((result (loop-1 (cadar config) (cdr path))))
                       (if (eq? result '()) (cdr config)
                           (cons (list (car path) result) (cdr config)))))
                    (else (cons (car config) (loop-2 (cdr config)))))))))))

  (define-method (~head self path (index -1))
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (window ((self '~config-get) '(public window)))
           (selected (cond ((null? path) -1) ((>= (car path) 0) (car path)) (else (+ index 1 (car path)))))
           (perm-info
            ((standard 'deep-call) ((self '~field!) 'perm) '()
             `(lambda (chain) (list ((chain 'index) ,selected)
                                    (- ((chain 'size)) 1) ((chain 'previous) ,index))))))
      (if (and window (<= (car perm-info) (- (cadr perm-info) window))) (caddr perm-info)
          (let ((temp-head ((self '~child) ((self '~field!) 'temp) 'previous index)))
            (if (and (equal? ((standard 'deep-get) temp-head path) '(unknown))
                     (equal? (sync-digest (caddr perm-info))
                             (sync-digest temp-head)))
                (caddr perm-info) temp-head)))))

  (define-method (~signature-sign! self chain public-key secret-key rotation)
    ;; Embed journal/interface public keys and journal signature into chain head using ephemeral step keys.
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (identity ((self '~config-get) '(public identity)))
           (latest-rotation-index
            (if (null? rotation)
                ((self '~config-get) '(public journal latest-rotation-index))
                (cadr (assoc 'index rotation))))
           (interface-public-key ((self '~config-get) '(public interface public-key)))
           (interface-endpoint ((self '~config-get) '(public interface endpoint)))
           (journal-name ((self '~config-get) '(public name))))
      (if (not (null? interface-endpoint))
          (set! chain ((standard 'deep-set!) chain '(-1 (*crypto* interface endpoint)) interface-endpoint)))
      (if (not (null? journal-name))
          (set! chain ((standard 'deep-set!) chain '(-1 (*crypto* journal name)) journal-name)))
      (set! chain ((standard 'deep-set!) chain '(-1 (*crypto* public-key)) #u()))
      (set! chain ((standard 'deep-set!) chain '(-1 (*crypto* signature)) #u()))
      (set! chain ((standard 'deep-set!) chain '(-1 (*crypto* journal identity id))
                   (cadr (assoc 'id identity))))
      (set! chain ((standard 'deep-set!) chain '(-1 (*crypto* journal identity nonce))
                   (cadr (assoc 'nonce identity))))
      (set! chain
            ((standard 'deep-set!) chain
             '(-1 (*crypto* journal latest-rotation-index))
             latest-rotation-index))
      (if (not (null? rotation))
          (set! chain
                ((standard 'deep-set!) chain
                 '(-1 (*crypto* journal rotation)) rotation)))
      (set! chain ((standard 'deep-set!) chain '(-1 (*crypto* journal public-key)) #u()))
      (set! chain ((standard 'deep-set!) chain '(-1 (*crypto* journal signature)) #u()))
      (set! chain ((standard 'deep-set!) chain '(-1 (*crypto* interface public-key)) interface-public-key))
      (let ((signature (crypto-sign secret-key (sync-digest chain))))
        ((standard 'deep-call!) chain '(-1)
         `(lambda (tree)
            ((tree 'set!) '(*crypto* public-key) ,public-key)
            ((tree 'set!) '(*crypto* signature) ,signature)
            ((tree 'set!) '(*crypto* journal public-key) ,public-key)
            ((tree 'set!) '(*crypto* journal signature) ,signature))))))

)
