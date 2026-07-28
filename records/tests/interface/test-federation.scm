(lambda (make-interface-harness)

  (with-let (make-interface-harness
             :journals 5 :journal-start 8 :users '(alice alice-user bob) :tick 1)

  (define (exact journal expression)
    (raw journal expression))

  (define* (collect submitted-action (expect '*no-expect*))
    ;; Exact protocol values are data dependencies between otherwise declarative
    ;; phases. Settle prior work before collecting the one value needed next.
    (test-report)
    (if (eq? expect '*no-expect*)
        (test-submit submitted-action)
        (test-submit submitted-action :expect expect))
    (test-await))

  (define (info journal)
    (collect ((*anonymous* journal 'info))))

  (define error-result?
    (lambda (result)
      (and (pair? result) (eq? (car result) 'error))))


  (define (interface-key-count journal root-secret)
    (exact journal
           `(*call* ,root-secret
                     (lambda (root)
                       (let* ((federation
                               (sync-eval
                                ((root 'get) '(root object federation))))
                              (keys (cadr (assoc 'keys
                                                 ((federation 'config))))))
                         (if (not (equal? ((root 'get) '(interface keys))
                                         '(nothing)))
                             (error 'integrity-error
                                    "Legacy Root interface keys remain authoritative"))
                         (length keys))))))

  (define* (signed-head journal root-secret (known-index -1))
    (collect
      (exact journal
              `(*call* ,root-secret
                       (lambda (root)
                         (((sync-eval ((root 'get) '(root object ledger)))
                           'signed-head) ,known-index))))))

  (define (reciprocal-bridge journal remote)
    ((*journal* journal 'bridge!) remote))

  (define (bulk-advance journal count)
    (raw journal
      `(*call* ,(journal 'root-secret)
        (lambda (root)
          (let* ((ledger (sync-eval ((root 'get) '(root object ledger))))
                 (identity-id
                  (cadr
                   (assoc 'id
                          (cadr
                           (assoc 'identity
                                  (cadr
                                   (assoc 'public ((ledger 'config)))))))))
                 (keys
                  (crypto-generate
                   (expression->byte-vector
                    (list 'sync-web/journal-signing-key/v1
                          identity-id
                          (sync-hash
                           (expression->byte-vector
                            ,(journal 'root-secret))))))))
            (let loop ((i 0))
              (if (< i ,count)
                  (begin
                    ((ledger 'set!) '(*state* bootstrap tick)
                     (expression->byte-vector i))
                    ((ledger 'step!)
                     `((unix-time ,i)
                       (public-key ,(car keys))
                       (secret-key ,(cdr keys))))
                    (loop (+ i 1)))))
            ((root 'set!) '(root object ledger) (ledger))
            ((ledger 'size)))))))

  (define (grant journal owner principal path)
    ((*journal* journal 'authorize!)
     `((user ,owner)
       (rule ((principal ,principal)
              (key-index (-10 -1))
              (path ,path)
              (get #t)
              (set! #t)
              (resolve #t))))))

  (define interface-8 (journal-8 'url))
  (define interface-11 (journal-11 'url))
  (define interface-12 (journal-12 'url))

  ;; Exact protocol tests below occasionally consume prior results to build a
  ;; later signed envelope, so settle automatic installation first.
  (test-report)

  (test-submit ((*anonymous* journal-8 'info)) :expect (lambda (result) (not (assoc 'bridge-policy result))))
  (test-submit ((*journal* journal-8 'config) '((path (private subscriber)))) :expect '())
  (test-submit ((*anonymous* journal-8 'synchronize)) :expect error-result?)
  (test-submit ((*journal* journal-8 'delete-subscriber!) '((name old))) :expect error-result?)

  ;; A bare remote identity is malformed: federation requires an exact committed
  ;; terminal object, not the removed signature-only authentication shape.
  (test-submit
      (exact journal-8
              '((function get)
                (arguments ((path (*state* alice seed)) (expression? #t)))
                (authentication ((identity (journal-9 *state* alice))))))
      :expect error-result?)

  ;; Establish signed initial heads before reciprocal bridge state is exchanged.
  (test-submit ((*journal* journal-8 'set!) :path '(*state* alice seed) :value "a") :expect #t)
  (test-submit ((*journal* journal-9 'set!) :path '(*state* carol seed) :value "c") :expect #t)
  (test-submit ((*journal* journal-10 'set!) :path '(*state* bob shared) :value "initial") :expect #t)
  (test-submit
    ((*journal* journal-10 'set!) :path '(*state* bob published)
     :value "public")
    :expect #t)
  (test-submit ((*journal* journal-10 'set!) :path '(*state* bob hidden) :value "hidden") :expect #t)
  (test-submit ((*journal* journal-10 'set!) :path '(*state* bob *directory*) :value #t) :expect #t)
  (test-submit ((*journal* journal-10 'set!) :path '(*state* nested data public key) :value "public nested") :expect #t)
  (test-submit
    ((*journal* journal-10 'set!)
     :path '(*state* nested data private key) :value "private nested")
    :expect #t)
  (test-submit ((*journal* journal-10 'set!) :path '(*state* nested data hidden key) :value "hidden nested") :expect #t)
  (test-submit ((*journal* journal-8 'step!)) :expect 1)
  (test-submit ((*journal* journal-9 'step!)) :expect 1)
  (test-submit ((*journal* journal-10 'step!)) :expect 1)
  (test-submit ((alice journal-8 'get) '(*state* alice seed)) :expect "a")
  (test-submit ((*journal* journal-11 'set!) :path '(*state* d seed) :value "d") :expect #t)
  (test-submit ((*journal* journal-12 'set!) :path '(*state* e seed) :value "e") :expect #t)
  (test-submit ((*journal* journal-11 'step!)) :expect 1)
  (test-submit ((*journal* journal-12 'step!)) :expect 1)

  ;; Locked-down acceptors reject unmatched creation without pending state,
  ;; then accept the exact admin-preapproved root key.
  (test-submit ((*journal* journal-11 'update-config!) '((path (public bridge-accept)) (value preapproved))) :expect #t)
  (test-submit (reciprocal-bridge journal-12 journal-11) :expect error-result?)
  (test-submit ((*journal* journal-11 'config) '((path (private bridge journal-12)))) :expect '())
  (let ((identity-id
         (cadr (assoc 'id (cadr (assoc 'identity (info journal-12)))))))
    (test-submit
      ((*journal* journal-11 'update-config!)
       `((path (private bridge-preapproval journal-12)) (value ,identity-id)))
      :expect #t))
  (let ((malformed-info
         (map (lambda (entry)
                (if (eq? (car entry) 'identity)
                    '(identity ((id #u(0 1 2)) (nonce #u(3 4 5))))
                    entry))
              (info journal-12))))
    (test-submit
      (exact journal-11
             `((function synchronize!)
               (arguments ((name journal-12)
                           (response ,(signed-head journal-12 "pass-12"))
                           (info ,malformed-info)
                           (interface ,interface-12)
                           (remote-name journal-11)))))
      :expect error-result?))
  (test-submit (exact journal-11
                 `((function synchronize!)
                   (arguments ((name journal-12)
                               (response ,(signed-head journal-12 "pass-12"))
                               (info ,(info journal-12))
                               (interface "https://attacker.invalid/interface")
                               (remote-name journal-11))))) :expect error-result?)
  (test-submit ((*journal* journal-11 'config) '((path (private bridge journal-12)))) :expect '())
  (test-submit (reciprocal-bridge journal-12 journal-11) :expect #t)
  (test-submit ((*journal* journal-11 'config) '((path (private bridge journal-12 initiation)))) :expect 'remote)
  (define retired-principal '(journal-12 *state* eve))
  (test-submit (grant journal-11 '(*state* d) retired-principal '(seed)) :expect #t)
  (test-submit ((*journal* journal-11 '*admins-set*) `((admins (,retired-principal)))) :expect error-result?)
  (test-submit ((*journal* journal-11 'delete-bridge!) 'journal-12) :expect #t)
  (test-submit ((*journal* journal-11 'authorizations) '((user (*state* d)))) :expect '())
  (test-submit ((*journal* journal-11 '*admins-get*)) :expect '())
  (test-submit ((*journal* journal-11 'config) '((path (private bridge-preapproval journal-12)))) :expect '())
  ;; A matching active initiator repairs the deleted reciprocal side even
  ;; under preapproval policy, and the
  ;; retained alias/root tombstone rejects identity substitution.
  (test-submit (reciprocal-bridge journal-12 journal-11) :expect #t)
  (test-submit
    ((*journal* journal-11 'config)
     '((path (private bridge journal-12 public-key))))
    :expect (cadr (assoc 'public-key (info journal-12))))
  (test-submit ((*journal* journal-11 'delete-bridge!) 'journal-12) :expect #t)
  (test-submit ((*journal* journal-11 'update-config!) '((path (public bridge-accept)) (value auto))) :expect #t)
  (test-submit (exact journal-11
                 `((function synchronize!)
                   (arguments ((name journal-12)
                               (response ,(signed-head journal-8 "pass-8"))
                               (info ,(info journal-8))
                               (interface ,interface-8)
                               (remote-name journal-11))))) :expect error-result?)
  (test-submit (reciprocal-bridge journal-12 journal-11) :expect #t)
  (test-submit (exact journal-11
                 `((function synchronize!)
                   (arguments ((name other)
                               (response ,(signed-head journal-12 "pass-12"))
                               (info ,(info journal-12))
                               (interface ,interface-12)
                               (remote-name journal-11))))) :expect error-result?)

  ;; Deleting the initiator side preserves identity tombstones and the peer's
  ;; last accepted local index. The identical relationship can then prove
  ;; continuity again after crossing log-chain layout boundaries.
  (test-submit (bulk-advance journal-12 31) :expect 32)
  ;; Model an in-flight synchronization accepted after the initiator's last
  ;; acknowledged remote-index. Its response is deliberately not applied at
  ;; the initiator before deletion.
  (test-submit
    (exact journal-11
           `((function synchronize!)
             (arguments ((name journal-12)
                         (response ,(signed-head journal-12 "pass-12" 0))
                         (interface ,interface-12)
                         (remote-name journal-11)))))
    :expect (lambda (result) (and (assoc 'ok? result)
                                  (cadr (assoc 'ok? result)))))
  (test-submit
    ((*journal* journal-11 'config)
     '((path (private bridge journal-12 last-index))))
    :expect 31)
  (test-submit ((*journal* journal-12 'delete-bridge!) 'journal-11) :expect #t)
  (test-submit
    ((*journal* journal-12 'config)
     '((path (private bridge-retired journal-11 head-index))))
    :expect 31)
  (test-submit ((*journal* journal-12 'step!)) :expect 33)
  (test-submit (reciprocal-bridge journal-12 journal-11) :expect #t)
  (test-submit ((*journal* journal-12 'config) '((path (private bridge journal-11 initiation)))) :expect 'local)
  (test-submit (reciprocal-bridge journal-12 journal-11) :expect #t)

  ;; A later deletion refreshes the retained continuity hint rather than
  ;; falling back to the older accepted index.
  (test-submit (bulk-advance journal-12 31) :expect 64)
  (test-submit ((*journal* journal-12 'delete-bridge!) 'journal-11) :expect #t)
  (test-submit ((*journal* journal-12 'step!)) :expect 65)
  (test-submit (reciprocal-bridge journal-12 journal-11) :expect #t)
  (test-submit ((*journal* journal-12 'config) '((path (private bridge-retired journal-11)))) :expect '())

  ;; Bridge creation performs the first reciprocal signed-head exchange and
  ;; records the inverse local-name mapping on the acceptor automatically.
  (test-submit (reciprocal-bridge journal-8 journal-9) :expect #t)
  (test-submit (reciprocal-bridge journal-9 journal-10) :expect #t)
  (test-submit ((*journal* journal-8 'config) '((path (private bridge journal-9 initiation)))) :expect 'local)
  (test-submit ((*journal* journal-9 'config) '((path (private bridge journal-8 initiation)))) :expect 'remote)
  (test-submit ((*journal* journal-9 'config) '((path (private bridge journal-10 initiation)))) :expect 'local)
  (test-submit ((*journal* journal-10 'config) '((path (private bridge journal-9 initiation)))) :expect 'remote)

  (define remote-principal '(journal-9 journal-8 *state* alice-user))
  (test-submit (grant journal-10 '(*state* bob) remote-principal '(shared)) :expect #t)
  (test-submit
    ((*journal* journal-10 'authorize!)
     '((user (*state* bob))
       (rule ((principal (*public*)) (path (published))
              (get #t) (set! #f) (resolve #t)))))
    :expect #t)
  (test-submit (grant journal-10 '(*state* nested) remote-principal '(data private)) :expect #t)
  (test-submit
    ((*journal* journal-10 'authorize!)
     '((user (*state* nested))
       (rule ((principal (*public*)) (path (data public))
              (get #t) (set! #f) (resolve #t)))))
    :expect #t)
  (test-submit ((*anonymous* journal-10 'get) '(*state* bob)) :expect (lambda (result)
            (and (eq? (car result) 'directory)
                 (= (length (cadr result)) 3)
                 (assoc 'shared (cadr result))
                 (assoc 'hidden (cadr result))
                 (assoc 'published (cadr result))
                 (not (assoc '*directory* (cadr result))))))
  (test-submit ((*anonymous* journal-10 'get) '(*state* bob hidden)) :expect error-result?)

  ;; Commit a state graph with intentionally receding nested checkpoints.
  ;; The creation exchanges already staged journal-8 and journal-10 heads in journal-9.
  (test-submit ((*journal* journal-9 'step!)) :expect 2)
  (test-submit ((*journal* journal-8 'step!)) :expect 2)
  (test-submit ((*journal* journal-9 'step!)) :expect 2)
  (test-submit ((*journal* journal-10 'step!)) :expect 2)
  (test-submit ((alice-user journal-8 journal-9 journal-10 'get) '(*state* bob shared)) :expect error-result?)
  ;; Propagate journal-10's committed reverse view back into an auditable journal-8 head.
  (test-submit ((*journal* journal-9 'step!)) :expect 3)
  (test-submit ((*journal* journal-8 'step!)) :expect 3)
  (test-submit
    ((*journal* journal-8 'resolve)
     '(-1 journal-9 -1 journal-10 *state* bob shared))
    :expect error-result?)
  (test-submit ((*anonymous* journal-8 'pin!) '(-1 journal-9)) :expect error-result?)
  (test-submit ((*anonymous* journal-8 'unpin!) '(-1 journal-9)) :expect error-result?)

  ;; Finite routes may return to a journal already visited. Policy can reject a
  ;; self-targeted operation, but the routing plumbing preserves the exact path.
  (collect
    ((*anonymous* journal-8 'route)
     '((route-target (journal-9 journal-8))))
    :expect
    (lambda (result)
      (and (equal? (cadr (assoc 'route-source result)) '(journal-8 journal-9))
           (list? (cadr (assoc 'object result))))))
  (test-submit (grant journal-8 '(*state* alice) remote-principal '(seed)) :expect #t)
  (test-submit ((alice-user journal-8 journal-9 journal-8 'get) '(*state* alice seed)) :expect "a")

  ;; One public routed read returns journal-10's exact committed object containing
  ;; journal-10-relative access back to journal-8's interface key.
  (let ((routed
         (collect
           ((*anonymous* journal-8 'route)
            '((route-target (journal-9 journal-10))))
           :expect
           (lambda (result)
             (and (equal? (cadr (assoc 'route-source result))
                          '(journal-8 journal-9))
                  (integer? (cadr (assoc 'terminal-index result)))
                  (list? (cadr (assoc 'object result))))))))
    (test-submit (exact journal-10
            `(*call* "pass-10"
                     (lambda (root)
                       (let ((ledger (sync-eval ((root 'get) '(root object ledger))))
                             (standard (sync-eval ((root 'get) '(root object standard)))))
                         (sync-node?
                          ((ledger 'read)
                           ,(cadr (assoc 'terminal-index routed))
                           ((standard 'deserialize)
                            ',(cadr (assoc 'object routed))))))))) :expect #t)
    (test-submit (exact journal-10
                   `((function get)
                     (arguments ((path (*state* bob shared)) (expression? #t)))
                     (invocation
                      ((identity alice-user)
                       (route-source ,(cadr (assoc 'route-source routed)))
                       (route-target ())
                       (object ,(cadr (assoc 'object routed)))
                       (terminal-index ,(cadr (assoc 'terminal-index routed)))
                       (audience ,(cadr (assoc 'public-key
                                               (info journal-10))))
                       (signature #u(1 2 3)))))) :expect error-result?))

  ;; The origin authenticates its local scalar identity, signs inside the journal,
  ;; and delivers the private function/arguments directly to the terminal endpoint.
  (test-submit ((alice-user journal-8 journal-9 journal-10 'get) '(*state* bob shared)) :expect "initial")
  ;; A proof-backed Tree-native value verifies across the direct route.
  (test-submit
    ((bob journal-9 journal-10 'resolve)
     '(-1 *state* bob published) :pinned? #t :proof? #t)
    :expect
    (lambda (result)
      (and (equal? (cadr (assoc 'content result)) "public")
           (eq? (cadr (assoc 'pinned? result)) #f)
           (list? (cadr (assoc 'proof result))))))
  (test-submit
    ((alice-user journal-8 journal-9 journal-10 'resolve
                 :history '(-1 -1 -1))
     '(-1 *state* nested) :pinned? #f :proof? #f)
    :expect (lambda (result)
              (and (eq? (car result) 'directory)
                   (assoc 'data (cadr result)))))
  (test-submit
    ((alice-user journal-8 journal-9 journal-10 'resolve
                 :history '(-1 -1 -1))
     '(-1 *state* nested) :pinned? #f :proof? #t)
    :expect (lambda (result)
              (and (eq? (car (cadr (assoc 'content result))) 'directory)
                   (assoc 'data (cadr (cadr (assoc 'content result))))
                   (list? (cadr (assoc 'proof result))))))
  (let* ((projected
          (collect
            ((alice-user journal-8 journal-9 journal-10 'resolve
                         :history '(-1 -1 -1))
             '(-1 *state* nested data) :pinned? #f :proof? #t)))
         (proof (cadr (assoc 'proof projected))))
    (test-submit
      (exact journal-8
              `(*call* "pass-8"
                       (lambda (root)
                         (let ((ledger (sync-eval ((root 'get) '(root object ledger))))
                               (standard (sync-eval ((root 'get) '(root object standard)))))
                           ((ledger '~resolve)
                            '(-1 journal-9 -1 journal-10
                                *state* nested data hidden key)
                            #f #f ((standard 'deserialize) ',proof))))))
      :expect '(unknown))
    (test-submit
      ((alice-user journal-8 journal-9 journal-10 'get)
       '(*state* nested data hidden key))
      :expect error-result?))
  (test-submit ((alice-user journal-8 journal-9 journal-10 'set!) '(*state* bob shared) "federated") :expect #t)

  (test-submit ((*journal* journal-10 'get) :path '(*state* bob shared) :expression? #t) :expect "federated")

  ;; Only state-path get, set!, and resolve may use a signed federated
  ;; application route. Transition history and all protocol/admin operations
  ;; remain local to their journal.
  (test-submit
    ((alice-user journal-8 journal-9 journal-10 'get)
     '(*transition* operation))
    :expect error-result?)
  (test-submit
    ((alice-user journal-8 journal-9 journal-10 'resolve)
     '(-1 *transition* operation) :pinned? #f :proof? #f)
    :expect error-result?)
  (test-submit ((alice-user journal-8 journal-9 journal-10 'info)) :expect error-result?)
  (test-submit ((alice-user journal-8 journal-9 journal-10 'size)) :expect error-result?)
  (test-submit
    ((alice-user journal-8 journal-9 journal-10 'batch!)
     '((queries
        (((function get)
          (arguments ((path (*state* bob shared)) (expression? #t))))
         ((function set!)
          (arguments ((path (*state* bob shared))
                      (value "batch") (expression? #t))))))))
    :expect error-result?)
  (test-submit
    ((alice-user journal-8 journal-9 journal-10 'set-batch!)
     '((paths ((*state* bob shared)))
       (values ("set-batch")) (expression? #t)))
    :expect error-result?)
  (test-submit ((alice-user journal-8 journal-9 journal-10 'bridge!) journal-11) :expect error-result?)
  (test-submit ((*journal* journal-10 'get) '(*state* bob shared)) :expect "federated")
  (test-submit ((*journal* journal-10 'config) '((path (private bridge david)))) :expect '())

  (test-submit ((*journal* journal-10 'step!)) :expect 3)
  ;; Committed reads expose only journal-10 state already incorporated into journal-8's view.
  (test-submit ((*journal* journal-9 'step!)) :expect 4)
  (test-submit ((*journal* journal-8 'step!)) :expect 4)

  (test-submit ((alice-user journal-8 journal-9 journal-10 'resolve) '(999 *state* bob shared)) :expect error-result?)
  (test-submit
    ((alice-user journal-8 journal-9 journal-10 'resolve)
     '(-1 *state* bob shared) :pinned? #f :proof? #f)
    :expect "federated")
  (let* ((projected
          (collect
            ((alice-user journal-8 journal-9 journal-10 'resolve)
             '(-1 *state* bob) :pinned? #f :proof? #t)))
         (proof (cadr (assoc 'proof projected))))
    (test-submit
      (exact journal-8
              `(*call* "pass-8"
                       (lambda (root)
                         (let ((ledger (sync-eval ((root 'get) '(root object ledger))))
                               (standard (sync-eval ((root 'get) '(root object standard)))))
                           ((ledger '~resolve)
                            '(-1 journal-9 -1 journal-10 *state* bob hidden)
                            #f #f ((standard 'deserialize) ',proof))))))
      :expect '(unknown))
    (test-submit
      (exact journal-8
              `(*call* "pass-8"
                       (lambda (root)
                         (let ((ledger (sync-eval ((root 'get) '(root object ledger))))
                               (standard (sync-eval ((root 'get) '(root object standard)))))
                           ((ledger '~resolve)
                            '(-1 journal-9 -1 journal-10 *state* bob *directory*)
                            #f #f ((standard 'deserialize) ',proof))))))
      :expect '(unknown)))
  ;; Authentication follows the canonical latest route while Ledger history
  ;; independently selects an older terminal checkpoint in the same traversal.
  (test-submit
    ((alice-user journal-8 journal-9 journal-10 'resolve :history '(-1 -1 1))
     '(-1 *state* bob shared) :pinned? #f :proof? #f)
    :expect "initial")
  (test-submit ((alice-user journal-8 journal-9 journal-10 'pin!) '(-1 *state* bob shared)) :expect error-result?)
  (test-submit ((alice-user journal-8 journal-9 journal-10 'unpin!) '(-1 *state* bob shared)) :expect error-result?)

  ;; Retention of remote content is an origin-local operation over the full path.
  (let* ((path '(-1 journal-9 -1 journal-10 *state* bob shared))
         (resolved
          (collect
            ((alice-user journal-8 journal-9 journal-10 'resolve)
             '(-1 *state* bob shared) :pinned? #f :proof? #t)))
         (proof (cadr (assoc 'proof resolved))))
    (test-submit ((*journal* journal-8 'pin!) path :response proof) :expect #t)
    (test-submit
      ((alice-user journal-8 journal-9 journal-10 'resolve)
       '(-1 *state* bob shared) :pinned? #t :proof? #f)
      :expect '((content "federated") (pinned? #t)))
    (test-submit ((*journal* journal-8 'unpin!) path) :expect #t))

  ;; Only the bridge initiator schedules the reciprocal exchange. The acceptor
  ;; stores the initiator head and later commits it during its own local step.
  (test-submit ((*journal* journal-8 'set!) :path '(*state* alice scheduled) :value "next") :expect #t)
  (test-submit ((*journal* journal-8 'step!)) :expect 5)
  (test-submit ((*journal* journal-9 'config) '((path (private bridge journal-8 last-index)))) :expect 3)

  ;; A remote rule without a signing-key window is rejected before it can
  ;; become an unusable policy entry.
  (test-submit
    ((*journal* journal-10 'authorize!)
     `((user (*state* bob))
       (rule ((principal ,remote-principal) (path (other))
              (get #t) (set! #f) (resolve #f)))))
    :expect error-result?)
  (test-submit ((alice-user journal-8 journal-9 journal-10 'get) '(*state* bob other)) :expect error-result?)

  ;; Interface-key rotation uses the key contained in journal-10's committed view.
  ;; A newly committed key is unusable until reciprocal synchronization carries
  ;; it into journal-10 and then carries that head back into journal-8 history.
  (define interface-8-secret-new "federation-a-new-secret")
  (test-submit ((*journal* journal-8 '*secret*) `((secret ,interface-8-secret-new))) :expect #t)
  (test-submit (interface-key-count journal-8 "pass-8") :expect 2)
  (test-submit (exact journal-8
                 `((function get)
                   (arguments ((path (*state* bob shared)) (expression? #t)))
                   (invocation ((identity alice-user)
                                (route-source ())
                                (route-target (journal-9 journal-10))
                                (credentials ,interface-8-secret-new))))) :expect "federated")
  (test-submit (exact journal-8
                 `((function set!)
                   (arguments ((path (*state* alice rotated))
                               (value "new-key")
                               (expression? #t)))
                   (authentication ((credentials ,interface-8-secret-new))))) :expect #t)
  (test-submit ((*journal* journal-8 'step!)) :expect 6)
  (test-submit (interface-key-count journal-8 "pass-8") :expect 1)
  (test-submit (exact journal-8
                 `((function get)
                   (arguments ((path (*state* bob shared)) (expression? #t)))
                   (invocation ((identity alice-user)
                                (route-source ())
                                (route-target (journal-9 journal-10))
                                (credentials ,interface-8-secret-new)))))
    :expect error-result?)
  (test-submit (interface-key-count journal-8 "pass-8") :expect 1)

  (test-report)))
